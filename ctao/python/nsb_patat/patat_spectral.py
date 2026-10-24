"""Four-component Patat spectral radiance benchmark for NSB and nsb2."""

from __future__ import annotations

import hashlib
import json
import math
from dataclasses import asdict, dataclass, field
from datetime import UTC, datetime
from importlib.metadata import distribution, version
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import astropy.units as u
import numpy as np
from astropy.constants import c, h
from astropy.coordinates import AltAz, EarthLocation, SkyCoord
from astropy.time import Time
from numpy.typing import NDArray

from nsb_patat.patat import PatatSpectrum, acquire, extract_library, load_selected
from nsb_patat.spectral import photon_bands

ARCSEC2_PER_SR = (180.0 / math.pi * 3600.0) ** 2
PHOTON_UNIT = 1 / u.cm**2 / u.ns / u.sr


@dataclass(frozen=True)
class RadiancePrediction:
    observation_id: str
    model: str
    version: str
    git_commit: str
    predicted_radiance: float
    unit: str = "photons cm^-2 ns^-1 sr^-1"
    status: str = "conditional"
    active_components: tuple[str, ...] = ()
    missing_components: tuple[str, ...] = ()
    component_radiance: dict[str, float] = field(default_factory=dict)
    assumptions: tuple[str, ...] = ()
    reason: str = ""

    def to_dict(self) -> dict:
        return asdict(self)


def solar_photon_band_fraction(wave_min_nm: float, wave_max_nm: float) -> float:
    """Independent Rieke (2008) solar photon fraction for Starlight redistribution."""
    from nsb2.core.photometry import SolarSpectrumRieke2008

    wavelength, spectrum = SolarSpectrumRieke2008()
    wave_nm = wavelength.to_value(u.nm)
    photon = (spectrum * wavelength / (h * c)).to_value(1 / (u.s * u.cm**2 * u.nm))

    def integrate(lo: float, hi: float) -> float:
        mask = (wave_nm >= lo) & (wave_nm <= hi)
        if not np.any(mask):
            raise ValueError("Solar spectrum does not cover the requested band")
        return float(np.trapezoid(photon[mask], wave_nm[mask]))

    return integrate(wave_min_nm, wave_max_nm) / integrate(300.0, 650.0)


def observed_photon_radiance(
    spectrum: PatatSpectrum,
    *,
    wave_min_nm: float,
    wave_max_nm: float,
    calibration_fractional_sigma: float | None,
) -> dict[str, float | None]:
    """Convert Patat energy spectra to the common photon surface-radiance band."""
    wave_nm = spectrum.wavelength_angstrom / 10.0
    # erg s^-1 cm^-2 A^-1 arcsec^-2 -> W m^-2 sr^-1 nm^-1
    energy = spectrum.energy_radiance * (1.0e-7 * 1.0e4 * 10.0 * ARCSEC2_PER_SR)
    bands = photon_bands(
        wave_nm * u.nm,
        energy * (u.W / u.m**2 / u.sr / u.nm),
        [wave_min_nm, wave_max_nm] * u.nm,
        calibration_fractional_sigma=calibration_fractional_sigma,
    )
    value = float(bands.values[0].to_value(PHOTON_UNIT))
    cal = None
    if bands.calibration_covariance is not None:
        cal = float(np.sqrt(bands.calibration_covariance[0, 0].to_value(PHOTON_UNIT**2)))
    return {
        "radiance": value,
        "calibration_sigma": cal,
        "statistical_sigma": None,
    }


def radiance_errors(observed: NDArray[np.float64], predicted: NDArray[np.float64]) -> dict:
    obs = np.asarray(observed, dtype=float)
    pred = np.asarray(predicted, dtype=float)
    if obs.shape != pred.shape or obs.ndim != 1 or not len(obs):
        raise ValueError("equal nonempty 1D samples required")
    if not np.all(np.isfinite(obs + pred)) or np.any(obs <= 0):
        raise ValueError("Finite positive observations required")
    delta = pred - obs
    relative = delta / obs
    corr = None
    if len(obs) > 1 and np.ptp(obs) > 0 and np.ptp(pred) > 0:
        corr = float(np.corrcoef(obs, pred)[0, 1])
    return {
        "n": len(obs),
        "mean_signed_error": float(np.mean(delta)),
        "mae": float(np.mean(np.abs(delta))),
        "rmse": float(np.sqrt(np.mean(delta**2))),
        "median_absolute_error": float(np.median(np.abs(delta))),
        "mean_absolute_relative_error": float(np.mean(np.abs(relative))),
        "median_absolute_relative_error": float(np.median(np.abs(relative))),
        "mean_relative_error": float(np.mean(relative)),
        "residual_p025_p975": np.percentile(delta, [2.5, 97.5]).tolist(),
        "relative_residual_p025_p975": np.percentile(relative, [2.5, 97.5]).tolist(),
        "correlation": corr,
    }


def grouped_radiance_scores(
    observed: NDArray[np.float64],
    predictions: dict[str, NDArray[np.float64]],
    nights: NDArray[np.int64],
    *,
    repetitions: int = 2000,
    seed: int = 182,
) -> dict:
    """Night-cluster bootstrap of absolute and relative MAE; paired model deltas."""
    if not predictions or len(nights) != len(observed) or repetitions < 100:
        raise ValueError("Models, matching nights and >=100 bootstrap draws required")
    models = {name: radiance_errors(observed, value) for name, value in predictions.items()}
    groups = [np.flatnonzero(nights == night) for night in np.unique(nights)]
    abs_draws: dict[str, list[float]] = {name: [] for name in predictions}
    rel_draws: dict[str, list[float]] = {name: [] for name in predictions}
    if len(groups) >= 3:
        rng = np.random.default_rng(seed)
        for _ in range(repetitions):
            selected = rng.integers(0, len(groups), len(groups))
            ids = np.concatenate([groups[int(i)] for i in selected])
            for name, pred in predictions.items():
                abs_draws[name].append(float(np.mean(np.abs(pred[ids] - observed[ids]))))
                rel_draws[name].append(
                    float(np.mean(np.abs((pred[ids] - observed[ids]) / observed[ids])))
                )
    for name in predictions:
        models[name]["mae_95pct_night_bootstrap_ci"] = (
            np.percentile(abs_draws[name], [2.5, 97.5]).tolist() if abs_draws[name] else None
        )
        models[name]["mean_absolute_relative_error_95pct_night_bootstrap_ci"] = (
            np.percentile(rel_draws[name], [2.5, 97.5]).tolist() if rel_draws[name] else None
        )
    paired = {}
    names = list(predictions)
    for i, left in enumerate(names):
        for right in names[i + 1 :]:
            abs_diff = np.abs(predictions[left] - observed) - np.abs(predictions[right] - observed)
            rel_left = np.abs((predictions[left] - observed) / observed)
            rel_right = np.abs((predictions[right] - observed) / observed)
            dist_abs = np.array(abs_draws[left]) - np.array(abs_draws[right])
            dist_rel = np.array(rel_draws[left]) - np.array(rel_draws[right])
            paired[f"{left}_minus_{right}"] = {
                "mean_absolute_error_difference": float(np.mean(abs_diff)),
                "mean_absolute_relative_error_difference": float(np.mean(rel_left - rel_right)),
                "mae_ci95": np.percentile(dist_abs, [2.5, 97.5]).tolist()
                if len(dist_abs)
                else None,
                "relative_mae_ci95": np.percentile(dist_rel, [2.5, 97.5]).tolist()
                if len(dist_rel)
                else None,
            }
    return {
        "models": models,
        "paired": paired,
        "independent_nights": len(groups),
        "bootstrap_repetitions": repetitions,
        "seed": seed,
        "primary_metric": "mean_absolute_relative_error",
        "estimand": "exposure-weighted; night-cluster bootstrap",
        "small_cluster_warning": len(groups) < 20,
    }


class NsbFourComponentAdapter:
    """NSB zodiacal+airglow restricted band + scaled production Starlight + moon=0."""

    def __init__(self, commit: str, site: dict, receipt_path: Path, f_band: float):
        import nsb
        import siderust

        receipt = json.loads(receipt_path.read_text())
        if receipt["git_commit"] != commit or not receipt["source_clean"]:
            raise ValueError("NSB build receipt does not identify the configured clean commit")
        package = distribution("nsb-py")
        for name, expected in receipt["extension_sha256"].items():
            digest = hashlib.sha256(Path(str(package.locate_file(name))).read_bytes()).hexdigest()
            if digest != expected:
                raise ValueError("Installed NSB extension differs from the verified build")
        self.nsb = nsb
        self.siderust = siderust
        self.commit = commit
        self.f_band = float(f_band)
        self.observer = siderust.Observer(
            lon_deg=site["longitude_deg"], lat_deg=site["latitude_deg"], height_m=site["height_m"]
        )
        self.evaluator = nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky())

    def predict(
        self, spectrum: PatatSpectrum, *, wave_min_nm: float, wave_max_nm: float
    ) -> RadiancePrediction:
        nsb = self.nsb
        instant: datetime = Time(spectrum.mjd_obs, format="mjd", scale="utc").to_datetime(
            timezone=UTC
        )
        target = self.siderust.Direction(ra_deg=spectrum.ra_deg, dec_deg=spectrum.dec_deg)
        za = nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW
        band = self.evaluator.integrate_band_components(
            nsb.PointQuery(self.observer, instant, target, components=za),
            wave_min_nm,
            wave_max_nm,
        )
        components = {c.name: float(c.integrated_photons_cm2_ns_sr) for c in band.components}
        star = self.evaluator.evaluate(
            nsb.PointQuery(self.observer, instant, target, components=nsb.ComponentMask.STARLIGHT)
        )
        starlight = float(star.components[0].integrated_photons_cm2_ns_sr) * self.f_band
        moonlight = 0.0
        total = components["zodiacal"] + components["airglow"] + starlight + moonlight
        active = ("zodiacal", "airglow", "moonlight", "starlight")
        return RadiancePrediction(
            spectrum.observation_id,
            "NSB",
            str(nsb.__version__),
            self.commit,
            total,
            status="conditional",
            active_components=active,
            missing_components=(),
            component_radiance={
                "zodiacal": components["zodiacal"],
                "airglow": components["airglow"],
                "moonlight": moonlight,
                "starlight": starlight,
                "starlight_map_300_650": float(star.components[0].integrated_photons_cm2_ns_sr),
                "starlight_f_band": self.f_band,
            },
            assumptions=(
                "generic clear-sky atmosphere",
                "PALACE continuum airglow (lines excluded)",
                "production Starlight map scaled by solar-photon band fraction",
                "moonlight physically zero under selection",
            ),
            reason="Conditional four-component radiance in the Patat-matched 361.5-650 nm band.",
        )


class Nsb2FourComponentAdapter:
    """nsb2 zodiacal + SkyCalc airglow + faint Gaia ISL + moon=0 surface radiance."""

    def __init__(self, commit: str, site: dict):
        from nsb2.emitter.stars import from_gaia_dr3_map
        from nsb2.emitter.zodiacal import from_leinert1998

        direct_url = json.loads(distribution("nsb2").read_text("direct_url.json") or "{}")
        if direct_url.get("vcs_info", {}).get("commit_id") != commit:
            raise ValueError("Installed nsb2 VCS identity differs from the configured commit")
        self.commit = commit
        self.site = EarthLocation.from_geodetic(
            site["longitude_deg"] * u.deg, site["latitude_deg"] * u.deg, site["height_m"] * u.m
        )
        self.zodiacal = from_leinert1998()
        self.starlight = from_gaia_dr3_map()
        self._airglow: dict[float, Any] = {}

    def _airglow_source(self, sfu: float):
        from nsb2.emitter.airglow import from_eso_skycalc

        key = round(float(sfu), 1)
        if key not in self._airglow:
            self._airglow[key] = from_eso_skycalc(90 * u.km, float(sfu))
        return self._airglow[key]

    def _integrate(
        self,
        source: Any,
        frame: AltAz,
        coords: SkyCoord,
        wave_min_nm: float,
        wave_max_nm: float,
    ) -> float:
        from nsb2.core.spectral import Bandpass

        field, _ = source.query_direct(SimpleNamespace(origin=frame), coords, np.array([0.0]))
        wave = field.spectral_grid.wvl.to_value(u.nm)
        raw = field.spectral_grid(field.spectral_data)
        try:
            density = (raw[:, 0] * field.weights[0, 0]).to_value(1 / u.cm**2 / u.ns / u.sr / u.nm)
            mask = (wave >= wave_min_nm) & (wave <= wave_max_nm)
            return float(np.trapezoid(density[mask], wave[mask]))
        except Exception:
            anchors = [wave_min_nm]
            for point in (400.0, 550.0):
                if wave_min_nm < point < wave_max_nm:
                    anchors.append(point)
            anchors.append(wave_max_nm)
            bandpass = Bandpass(np.array(anchors) * u.nm, np.ones(len(anchors)))
            rate = field.resolve_spectra(bandpass).integrate()
            value = rate.to_value(
                1 / (u.s * u.cm**2 * u.sr), equivalencies=u.dimensionless_angles()
            )
            return float(np.asarray(value).reshape(-1)[0]) * 1.0e-9

    def predict(
        self, spectrum: PatatSpectrum, *, wave_min_nm: float, wave_max_nm: float
    ) -> RadiancePrediction:
        frame = AltAz(
            obstime=Time(spectrum.mjd_obs, format="mjd", scale="utc"),
            location=self.site,
            pressure=0 * u.hPa,
        )
        coords = SkyCoord(ra=[spectrum.ra_deg] * u.deg, dec=[spectrum.dec_deg] * u.deg)
        airglow = self._integrate(
            self._airglow_source(spectrum.sun_flux_sfu), frame, coords, wave_min_nm, wave_max_nm
        )
        zodiacal = self._integrate(self.zodiacal, frame, coords, wave_min_nm, wave_max_nm)
        starlight = self._integrate(self.starlight, frame, coords, wave_min_nm, wave_max_nm)
        moonlight = 0.0
        total = zodiacal + airglow + starlight + moonlight
        return RadiancePrediction(
            spectrum.observation_id,
            "nsb2",
            version("nsb2"),
            self.commit,
            total,
            status="conditional",
            active_components=("zodiacal", "airglow", "moonlight", "starlight"),
            missing_components=(),
            component_radiance={
                "zodiacal": zodiacal,
                "airglow": airglow,
                "moonlight": moonlight,
                "starlight": starlight,
            },
            assumptions=(
                "source-level surface radiance (no IACT instrument response)",
                "airglow SFU taken from Patat SUNFLUX header",
                "diffuse Gaia faint map only for starlight",
                "moonlight physically zero under selection",
            ),
            reason="Conditional four-component radiance matched to the Patat band.",
        )


def _split_nights(nights: NDArray[np.int64], *, fraction: float, seed: int) -> set[int]:
    unique = sorted(set(int(n) for n in nights))
    calibration: set[int] = set()
    for night in unique:
        digest = hashlib.sha256(f"{seed}:{night}".encode()).hexdigest()
        if int(digest[:8], 16) / 0xFFFFFFFF < fraction:
            calibration.add(night)
    if not calibration and unique:
        calibration.add(unique[0])
    return calibration


def run_benchmark(
    config_path: Path,
    data_dir: Path,
    output_dir: Path,
    nsb_build_receipt: Path,
) -> dict:
    """Execute the preregistered Patat four-component empirical benchmark."""
    config = json.loads(config_path.read_text())
    if output_dir.exists():
        raise FileExistsError(output_dir)
    output_dir.mkdir(parents=True, exist_ok=False)
    figures = output_dir / "figures"
    figures.mkdir()

    archive = data_dir / config["dataset"]["filename"]
    acquisition = acquire(archive, config["dataset"]["sha256"])
    library = extract_library(archive, data_dir / "library")
    selection = {
        **config["selection"],
        "wavelength_min_nm": config["observable"]["wavelength_min_nm"],
        "wavelength_max_nm": config["observable"]["wavelength_max_nm"],
    }
    spectra, selection_summary = load_selected(library, selection)
    if not spectra:
        raise RuntimeError("No spectra admitted by the registered selection")

    f_band = solar_photon_band_fraction(
        config["observable"]["wavelength_min_nm"], config["observable"]["wavelength_max_nm"]
    )
    config["models"]["nsb"]["starlight_band_fraction"]["f_band_365_650_over_300_650"] = f_band

    observations = []
    for spectrum in spectra:
        measured = observed_photon_radiance(
            spectrum,
            wave_min_nm=config["observable"]["wavelength_min_nm"],
            wave_max_nm=config["observable"]["wavelength_max_nm"],
            calibration_fractional_sigma=config["calibration_fractional_sigma"],
        )
        observations.append(
            {
                "observation_id": spectrum.observation_id,
                "civil_night": spectrum.civil_night,
                "mjd_obs": spectrum.mjd_obs,
                "ra_deg": spectrum.ra_deg,
                "dec_deg": spectrum.dec_deg,
                "airmass": spectrum.airmass,
                "galactic_latitude_deg": spectrum.galactic_latitude_deg,
                "ecliptic_latitude_deg": spectrum.ecliptic_latitude_deg,
                "alt_moon_deg": spectrum.alt_moon_deg,
                "sun_flux_sfu": spectrum.sun_flux_sfu,
                "observed_radiance": measured["radiance"],
                "calibration_sigma": measured["calibration_sigma"],
            }
        )

    site = config["dataset"]["site"]
    nsb = NsbFourComponentAdapter(
        config["models"]["nsb"]["commit"], site, nsb_build_receipt, f_band
    )
    nsb2 = Nsb2FourComponentAdapter(config["models"]["nsb2"]["commit"], site)
    nsb_predictions = []
    nsb2_predictions = []
    for spectrum in spectra:
        nsb_predictions.append(
            nsb.predict(
                spectrum,
                wave_min_nm=config["observable"]["wavelength_min_nm"],
                wave_max_nm=config["observable"]["wavelength_max_nm"],
            )
        )
        nsb2_predictions.append(
            nsb2.predict(
                spectrum,
                wave_min_nm=config["observable"]["wavelength_min_nm"],
                wave_max_nm=config["observable"]["wavelength_max_nm"],
            )
        )

    observed = np.array([row["observed_radiance"] for row in observations], dtype=float)
    nights = np.array([row["civil_night"] for row in observations], dtype=np.int64)
    pred = {
        "NSB": np.array([p.predicted_radiance for p in nsb_predictions], dtype=float),
        "nsb2": np.array([p.predicted_radiance for p in nsb2_predictions], dtype=float),
    }
    for name, values in pred.items():
        if not np.all(np.isfinite(values)):
            raise RuntimeError(f"{name} produced non-finite predictions")
        preds = nsb_predictions if name == "NSB" else nsb2_predictions
        if any(p.missing_components for p in preds):
            raise RuntimeError(f"{name} missing required components")

    calibration_nights = _split_nights(
        nights,
        fraction=config["statistics"]["calibration_night_fraction"],
        seed=config["statistics"]["seed"],
    )
    train = np.array([night in calibration_nights for night in nights])
    if not np.any(train) or not np.any(~train):
        raise RuntimeError("Calibration/evaluation night split is empty")
    baseline_value = float(np.median(observed[train]))
    baseline = np.full_like(observed, baseline_value)
    evaluation = ~train
    scores = grouped_radiance_scores(
        observed[evaluation],
        {
            "NSB": pred["NSB"][evaluation],
            "nsb2": pred["nsb2"][evaluation],
            "baseline": baseline[evaluation],
        },
        nights[evaluation],
        repetitions=config["statistics"]["bootstrap_repetitions"],
        seed=config["statistics"]["seed"],
    )

    paired = scores["paired"].get("NSB_minus_nsb2", {})
    rel_diff = paired.get("mean_absolute_relative_error_difference")
    rel_ci = paired.get("relative_mae_ci95")
    threshold = config["statistics"]["practically_meaningful_relative_mae_difference"]
    if rel_diff is None or rel_ci is None:
        superiority = "comparison_incomplete"
    elif rel_ci[0] > threshold:
        superiority = "nsb2_performs_better"
    elif rel_ci[1] < -threshold:
        superiority = "NSB_performs_better"
    elif rel_ci[0] <= 0 <= rel_ci[1]:
        superiority = "no_demonstrated_difference"
    else:
        superiority = "difference_not_practically_meaningful"

    admission = {
        "mode": "conditional",
        "admitted_observations": len(observations),
        "admitted_nights": len(set(nights.tolist())),
        "evaluation_observations": int(evaluation.sum()),
        "evaluation_nights": len(set(nights[evaluation].tolist())),
        "calibration_nights": sorted(calibration_nights),
        "four_components_present": True,
        "moonlight_mode": "physical_zero_under_selection",
        "absolute_claim": False,
        "conditional_claim": True,
        "superiority": superiority,
        "limitations": config["admission"]["known_limitations"],
    }

    # Figures
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=(5.5, 5))
    ax.scatter(observed[evaluation], pred["NSB"][evaluation], s=18, alpha=0.8, label="NSB")
    ax.scatter(observed[evaluation], pred["nsb2"][evaluation], s=18, alpha=0.8, label="nsb2")
    hi = max(
        float(observed[evaluation].max()),
        float(pred["NSB"][evaluation].max()),
        float(pred["nsb2"][evaluation].max()),
    )
    lims = [0.0, hi * 1.05]
    ax.plot(lims, lims, color="black", lw=1)
    ax.set_xlabel("Observed radiance [ph cm$^{-2}$ ns$^{-1}$ sr$^{-1}$]")
    ax.set_ylabel("Predicted radiance")
    ax.set_title("Patat 365-650 nm four-component benchmark")
    ax.legend()
    fig.tight_layout()
    fig.savefig(figures / "predicted-observed.png", dpi=140)
    fig.savefig(figures / "predicted-observed.svg")
    plt.close(fig)

    rel_nsb = (pred["NSB"][evaluation] - observed[evaluation]) / observed[evaluation]
    rel_nsb2 = (pred["nsb2"][evaluation] - observed[evaluation]) / observed[evaluation]
    fig, ax = plt.subplots(figsize=(6, 4))
    ax.hist(rel_nsb, bins=15, alpha=0.6, label="NSB")
    ax.hist(rel_nsb2, bins=15, alpha=0.6, label="nsb2")
    ax.axvline(0, color="black", lw=1)
    ax.set_xlabel("Relative residual (pred-obs)/obs")
    ax.set_ylabel("Exposures")
    ax.legend()
    fig.tight_layout()
    fig.savefig(figures / "relative-residuals.png", dpi=140)
    fig.savefig(figures / "relative-residuals.svg")
    plt.close(fig)

    gal = np.asarray([s.galactic_latitude_deg for s in spectra], dtype=np.float64)
    fig, ax = plt.subplots(figsize=(6, 4))
    ax.scatter(gal[evaluation], rel_nsb, s=18, alpha=0.8, label="NSB")
    ax.scatter(gal[evaluation], rel_nsb2, s=18, alpha=0.8, label="nsb2")
    ax.axhline(0, color="black", lw=1)
    ax.set_xlabel("Galactic latitude [deg]")
    ax.set_ylabel("Relative residual")
    ax.legend()
    fig.tight_layout()
    fig.savefig(figures / "residual-galactic.png", dpi=140)
    plt.close(fig)

    metrics = {
        "observable": config["observable"],
        "selection_summary": selection_summary,
        "f_band": f_band,
        "baseline_radiance": baseline_value,
        "scores": scores,
        "admission": admission,
        "component_means": {
            "NSB": {
                key: float(np.mean([p.component_radiance[key] for p in nsb_predictions]))
                for key in ("zodiacal", "airglow", "moonlight", "starlight")
            },
            "nsb2": {
                key: float(np.mean([p.component_radiance[key] for p in nsb2_predictions]))
                for key in ("zodiacal", "airglow", "moonlight", "starlight")
            },
        },
    }
    rows_out = []
    for i, _spectrum in enumerate(spectra):
        rows_out.append(
            {
                **observations[i],
                "split": "calibration" if train[i] else "evaluation",
                "NSB": nsb_predictions[i].to_dict(),
                "nsb2": nsb2_predictions[i].to_dict(),
                "baseline": baseline_value,
            }
        )

    manifest = {
        "campaign": config["campaign"],
        "created_utc": datetime.now(tz=UTC).isoformat(),
        "config_sha256": hashlib.sha256(config_path.read_bytes()).hexdigest(),
        "dataset": acquisition,
        "nsb_receipt_sha256": hashlib.sha256(nsb_build_receipt.read_bytes()).hexdigest(),
        "nsb_commit": config["models"]["nsb"]["commit"],
        "nsb2_commit": config["models"]["nsb2"]["commit"],
        "nsb2_version": version("nsb2"),
        "artifacts": {},
    }
    payloads = {
        "config.snapshot.json": config,
        "acquisition.json": acquisition,
        "selection.json": selection_summary,
        "admission.json": admission,
        "metrics.json": metrics,
        "observations.json": rows_out,
    }
    write_artifacts(output_dir, payloads, figures, manifest)
    return {"output": str(output_dir), "admission": admission, "scores": scores}


def write_artifacts(output_dir: Path, payloads: dict, figures: Path, manifest: dict) -> None:
    """Write payloads, then a non-self-referential manifest and detached digest."""
    manifest["artifacts"] = {}
    for name, payload in payloads.items():
        if name in {"manifest.json", "manifest.sha256"} or Path(name).name != name:
            raise ValueError("Invalid artifact payload name")
        path = output_dir / name
        path.write_text(json.dumps(payload, indent=2, allow_nan=False) + "\n")
        manifest["artifacts"][name] = hashlib.sha256(path.read_bytes()).hexdigest()
    for path in sorted(figures.glob("*")):
        if not path.is_file() or path.is_symlink():
            raise ValueError("Invalid figure artifact")
        manifest["artifacts"][f"figures/{path.name}"] = hashlib.sha256(
            path.read_bytes()
        ).hexdigest()
    manifest_path = output_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, allow_nan=False) + "\n")
    digest = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    (output_dir / "manifest.sha256").write_text(f"{digest}  manifest.json\n")
    verify_artifacts(output_dir)


def verify_artifacts(output_dir: Path) -> None:
    """Verify final manifest and every registered artifact; unknown is not trusted."""
    manifest_path = output_dir / "manifest.json"
    expected = (output_dir / "manifest.sha256").read_text().strip()
    digest = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    if expected != f"{digest}  manifest.json":
        raise ValueError("Manifest checksum mismatch")
    manifest = json.loads(manifest_path.read_text())
    expected_names = set(manifest["artifacts"]) | {"manifest.json", "manifest.sha256"}
    actual_names = {
        path.relative_to(output_dir).as_posix()
        for path in output_dir.rglob("*")
        if path.is_file() or path.is_symlink()
    }
    if expected_names != actual_names:
        raise ValueError("Artifact inventory mismatch")
    for name, checksum in manifest["artifacts"].items():
        relative = Path(name)
        if (
            relative.is_absolute()
            or relative.as_posix() != name
            or ".." in relative.parts
            or name in {"manifest.json", "manifest.sha256"}
        ):
            raise ValueError("Invalid manifest artifact name")
        path = output_dir / relative
        if (
            path.is_symlink()
            or not path.is_file()
            or hashlib.sha256(path.read_bytes()).hexdigest() != checksum
        ):
            raise ValueError(f"Artifact checksum mismatch: {name}")
