"""Full four-component Patat/FORS1 benchmark for NSB v0.2.0 vs nsb2.

Primary CTAO notebook experiment: total modeled photon surface radiance versus
total observed photon surface radiance over 365–650 nm, using the registered
selection and calibration/evaluation night partition.

Independence status: DEVELOPMENT_ONLY — the Patat cohort informed Oxygen
development and is not a new untouched holdout.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from importlib.metadata import distribution, version
from pathlib import Path
from typing import Any

import nsb
import numpy as np
import siderust
from astropy.time import Time

from nsb_patat.patat import acquire, extract_library, load_selected
from nsb_patat.patat_spectral import (
    Nsb2FourComponentAdapter,
    RadiancePrediction,
    _split_nights,
    grouped_radiance_scores,
    observed_photon_radiance,
    solar_photon_band_fraction,
    write_artifacts,
)

# Public main tip after #278 (v0.2.0 packaging) with Oxygen from #272.
PINNED_NSB_COMMIT = None  # filled by ctao/install_nsb.sh receipt / local HEAD
PINNED_NSB2_COMMIT = "bc9320db03fbff69997ce366c6e76a37339c5d00"

HISTORICAL_FOUR_COMPONENT_MAE = {
    "note": (
        "Frozen historical four-component scores from prior NSB/Oxygen development "
        "analyses. Not notebook outputs; use for discrepancy investigation only."
    ),
    "NSB_continuum_native": 0.36103,
    "NSB_oxygen_native": 0.23882,
    "NSB_continuum_F107": 0.35027,
    "NSB_oxygen_F107": 0.16573,
    "nsb2_matched_F107": 0.17425,
    "median_baseline": 0.15245,
}

SOLAR_CONFIG_SHA256 = "cd44cf6f2e162b763cfe3a216402f4a18d6b2ca88d05907496c2c2c738d225dc"
SOLAR_SOURCE_SHA256 = "7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96"


def _paired(scores: dict[str, Any], left: str, right: str) -> dict[str, Any] | None:
    """Return left−right paired stats regardless of dict insertion order."""
    direct = scores["paired"].get(f"{left}_minus_{right}")
    if direct is not None:
        return direct
    flipped = scores["paired"].get(f"{right}_minus_{left}")
    if flipped is None:
        return None
    return {
        "mean_absolute_error_difference": -flipped["mean_absolute_error_difference"],
        "mean_absolute_relative_error_difference": -flipped[
            "mean_absolute_relative_error_difference"
        ],
        "mae_ci95": (
            [-flipped["mae_ci95"][1], -flipped["mae_ci95"][0]]
            if flipped["mae_ci95"] is not None
            else None
        ),
        "relative_mae_ci95": (
            [-flipped["relative_mae_ci95"][1], -flipped["relative_mae_ci95"][0]]
            if flipped["relative_mae_ci95"] is not None
            else None
        ),
    }


def load_observed_f107_daily(path: Path) -> dict:
    """Load the frozen Patat diagnostic F10.7 slice (checksum-verified)."""
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != SOLAR_CONFIG_SHA256:
        raise ValueError("Frozen F10.7 diagnostic slice checksum mismatch")
    payload = json.loads(path.read_text())
    if payload["source_sha256"] != SOLAR_SOURCE_SHA256:
        raise ValueError("Frozen F10.7 source identity mismatch")
    return {
        datetime.strptime(day, "%Y-%m-%d").date(): float(value)
        for day, value in payload["daily"].items()
    }


def centered_27day_f107(daily: dict, day) -> float:
    days = [day + timedelta(days=i) for i in range(-13, 14)]
    missing = [d for d in days if d not in daily]
    if missing:
        raise ValueError(f"Incomplete centred 27-day observed F10.7 window: {missing[:3]}...")
    return float(np.mean([daily[d] for d in days]))


def verify_installed_nsb(*, expected_commit: str | None, receipt_path: Path | None) -> dict[str, Any]:
    """Verify installed nsb-py against an optional build receipt / expected commit."""
    package = distribution("nsb-py")
    info: dict[str, Any] = {
        "version": version("nsb-py"),
        "MODEL_VERSION": getattr(nsb, "MODEL_VERSION", None),
        "SIDERUST_VERSION": getattr(nsb, "SIDERUST_VERSION", None),
        "SIDERUST_SOURCE": getattr(nsb, "SIDERUST_SOURCE", None),
        "has_oxygen": hasattr(nsb.AirglowModel, "PARANAL_PALACE_OXYGEN_V1"),
        "has_continuum": hasattr(nsb.AirglowModel, "PARANAL_PALACE_V1"),
    }
    if not info["has_oxygen"] or not info["has_continuum"]:
        raise RuntimeError("Installed NSB lacks required AirglowModel members")
    if receipt_path is not None:
        receipt = json.loads(receipt_path.read_text())
        if expected_commit and receipt.get("git_commit") != expected_commit:
            raise ValueError("NSB build receipt commit differs from expected pin")
        if not receipt.get("source_clean", False):
            import os

            if os.environ.get("NSB_CTAO_ALLOW_DIRTY", "") != "1":
                raise ValueError(
                    "NSB build receipt is not marked source_clean "
                    "(set NSB_CTAO_ALLOW_DIRTY=1 for intentional dirty RC builds)"
                )
            info["commit_verification"] = "dirty_source_allowed"
        for name, expected in receipt.get("extension_sha256", {}).items():
            digest = hashlib.sha256(Path(str(package.locate_file(name))).read_bytes()).hexdigest()
            if digest != expected:
                raise ValueError(f"Installed NSB extension digest mismatch: {name}")
        info["receipt"] = {
            "path_name": receipt_path.name,
            "git_commit": receipt.get("git_commit"),
            "wheel_sha256": receipt.get("wheel_sha256"),
            "cargo_lock_sha256": receipt.get("cargo_lock_sha256"),
        }
        info["git_commit"] = receipt.get("git_commit")
    elif expected_commit:
        info["git_commit"] = expected_commit
        info["commit_verification"] = (
            "expected_commit recorded without local receipt — "
            "prefer build via ctao/reproduce.sh for full provenance"
        )
    else:
        info["git_commit"] = None
        info["commit_verification"] = "unverified_runtime"
    return info


class NsbV02FourComponentAdapter:
    """Complete NSB v0.2.0 four-component radiance (zodiacal+airglow+starlight+moon=0)."""

    def __init__(
        self,
        *,
        site: dict,
        f_band: float,
        airglow_model: Any,
        label: str,
        commit: str | None,
        solar_sfu: float | None = None,
    ):
        self.nsb = nsb
        self.siderust = siderust
        self.f_band = float(f_band)
        self.airglow_model = airglow_model
        self.label = label
        self.commit = commit or "unverified"
        self.solar_sfu = solar_sfu
        self.observer = siderust.Observer(
            lon_deg=site["longitude_deg"],
            lat_deg=site["latitude_deg"],
            height_m=site["height_m"],
        )
        cfg = nsb.NsbModelConfig.generic_clear_sky()
        if airglow_model is not None:
            cfg = cfg.with_airglow_model(airglow_model)
        if solar_sfu is not None:
            cfg = cfg.with_solar_radio_flux_sfu(float(solar_sfu))
        self.evaluator = nsb.NsbEvaluator(cfg)
        self._solar_mode = "fixed" if solar_sfu is not None else "native"

    def predict(
        self,
        spectrum,
        *,
        wave_min_nm: float,
        wave_max_nm: float,
        solar_sfu: float | None = None,
    ) -> RadiancePrediction:
        """Predict total band radiance; optional per-spectrum solar override."""
        nsb_mod = self.nsb
        if solar_sfu is None and self.solar_sfu is None:
            evaluator = self.evaluator
            solar_mode = "native"
        elif solar_sfu is None:
            evaluator = self.evaluator
            solar_mode = self._solar_mode
        else:
            cfg = nsb_mod.NsbModelConfig.generic_clear_sky()
            if self.airglow_model is not None:
                cfg = cfg.with_airglow_model(self.airglow_model)
            cfg = cfg.with_solar_radio_flux_sfu(float(solar_sfu))
            evaluator = nsb_mod.NsbEvaluator(cfg)
            solar_mode = "measured_f107"

        instant: datetime = Time(spectrum.mjd_obs, format="mjd", scale="utc").to_datetime(
            timezone=UTC
        )
        target = self.siderust.Direction(ra_deg=spectrum.ra_deg, dec_deg=spectrum.dec_deg)
        za = nsb_mod.ComponentMask.ZODIACAL | nsb_mod.ComponentMask.AIRGLOW
        band = evaluator.integrate_band_components(
            nsb_mod.PointQuery(self.observer, instant, target, components=za),
            wave_min_nm,
            wave_max_nm,
        )
        components = {c.name: float(c.integrated_photons_cm2_ns_sr) for c in band.components}
        star = evaluator.evaluate(
            nsb_mod.PointQuery(
                self.observer, instant, target, components=nsb_mod.ComponentMask.STARLIGHT
            )
        )
        starlight = float(star.components[0].integrated_photons_cm2_ns_sr) * self.f_band
        moonlight = 0.0
        total = components["zodiacal"] + components["airglow"] + starlight + moonlight
        airglow_name = (
            "automatic"
            if self.airglow_model is None
            else self.airglow_model.as_str()
        )
        return RadiancePrediction(
            spectrum.observation_id,
            self.label,
            str(nsb_mod.__version__),
            self.commit,
            total,
            status="conditional",
            active_components=("zodiacal", "airglow", "moonlight", "starlight"),
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
                f"airglow model: {airglow_name}",
                f"solar mode: {solar_mode}",
                "production Starlight map scaled by solar-photon band fraction",
                "moonlight physically zero under moonless selection",
            ),
            reason=(
                "Four-component ground-level photon radiance in the Patat-matched "
                "365-650 nm band (NSB v0.2.0 candidate)."
            ),
        )


def run_four_component_benchmark(
    *,
    config_path: Path,
    data_dir: Path,
    output_dir: Path,
    solar_path: Path,
    nsb_receipt: Path | None = None,
    expected_nsb_commit: str | None = None,
) -> dict[str, Any]:
    """Execute continuum/Oxygen/nsb2/baseline four-component Patat comparison."""
    config = json.loads(config_path.read_text())
    if output_dir.exists():
        raise FileExistsError(output_dir)
    output_dir.mkdir(parents=True, exist_ok=False)
    figures = output_dir / "figures"
    figures.mkdir()

    if expected_nsb_commit is None and nsb_receipt is not None and nsb_receipt.is_file():
        expected_nsb_commit = __import__("json").loads(nsb_receipt.read_text()).get("git_commit")
    nsb_info = verify_installed_nsb(
        expected_commit=expected_nsb_commit, receipt_path=nsb_receipt
    )
    nsb_commit = nsb_info.get("git_commit") or expected_nsb_commit or "unverified"

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

    daily = load_observed_f107_daily(solar_path)
    f_band = solar_photon_band_fraction(
        config["observable"]["wavelength_min_nm"], config["observable"]["wavelength_max_nm"]
    )
    wave_min = float(config["observable"]["wavelength_min_nm"])
    wave_max = float(config["observable"]["wavelength_max_nm"])
    cal = float(config.get("calibration_fractional_sigma", 0.05))
    site = config["dataset"]["site"]

    continuum = NsbV02FourComponentAdapter(
        site=site,
        f_band=f_band,
        airglow_model=nsb.AirglowModel.PARANAL_PALACE_V1,
        label="NSB_continuum",
        commit=nsb_commit,
    )
    # Primary v0.2.0 path: Automatic (resolves to Oxygen). Explicit Oxygen remains
    # available elsewhere for identity checks; continuum stays explicit-only.
    oxygen = NsbV02FourComponentAdapter(
        site=site,
        f_band=f_band,
        airglow_model=None,
        label="NSB_oxygen",
        commit=nsb_commit,
    )
    nsb2 = Nsb2FourComponentAdapter(config["models"]["nsb2"]["commit"], site)

    observations = []
    preds: dict[str, list[RadiancePrediction]] = {
        "NSB_continuum_native": [],
        "NSB_oxygen_native": [],
        "NSB_continuum_f27": [],
        "NSB_oxygen_f27": [],
        "nsb2_header": [],
        "nsb2_f27": [],
    }
    for spectrum in spectra:
        measured = observed_photon_radiance(
            spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max, calibration_fractional_sigma=cal
        )
        instant = Time(spectrum.mjd_obs, format="mjd", scale="utc").to_datetime(timezone=UTC)
        f27 = centered_27day_f107(daily, instant.date())
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
                "sun_flux_sfu_header": float(spectrum.sun_flux_sfu),
                "observed_f107_centered_27d": f27,
                "observed_radiance": measured["radiance"],
                "calibration_sigma": measured["calibration_sigma"],
            }
        )
        preds["NSB_continuum_native"].append(
            continuum.predict(spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max)
        )
        preds["NSB_oxygen_native"].append(
            oxygen.predict(spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max)
        )
        preds["NSB_continuum_f27"].append(
            continuum.predict(
                spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max, solar_sfu=f27
            )
        )
        preds["NSB_oxygen_f27"].append(
            oxygen.predict(spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max, solar_sfu=f27)
        )
        preds["nsb2_header"].append(
            nsb2.predict(spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max)
        )
        # Matched F10.7 for nsb2: re-query SkyCalc airglow with measured 27-day mean.
        spectrum_f27 = replace(spectrum, sun_flux_sfu=f27)
        preds["nsb2_f27"].append(
            nsb2.predict(spectrum_f27, wave_min_nm=wave_min, wave_max_nm=wave_max)
        )

    observed = np.array([row["observed_radiance"] for row in observations], dtype=float)
    nights = np.array([row["civil_night"] for row in observations], dtype=np.int64)
    pred_arrays = {
        name: np.array([p.predicted_radiance for p in plist], dtype=float)
        for name, plist in preds.items()
    }
    for name, values in pred_arrays.items():
        if not np.all(np.isfinite(values)):
            raise RuntimeError(f"{name} produced non-finite predictions")

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

    score_inputs = {**{k: v[evaluation] for k, v in pred_arrays.items()}, "baseline": baseline[evaluation]}
    scores = grouped_radiance_scores(
        observed[evaluation],
        score_inputs,
        nights[evaluation],
        repetitions=config["statistics"]["bootstrap_repetitions"],
        seed=config["statistics"]["seed"],
    )

    # Primary comparison table rows for the notebook executive summary
    primary_keys = (
        "NSB_continuum_native",
        "NSB_oxygen_native",
        "NSB_continuum_f27",
        "NSB_oxygen_f27",
        "nsb2_header",
        "nsb2_f27",
        "baseline",
    )
    primary_table = []
    for key in primary_keys:
        m = scores["models"][key]
        primary_table.append(
            {
                "configuration": key,
                "n_eval": int(evaluation.sum()),
                "n_eval_nights": len(set(nights[evaluation].tolist())),
                "mean_absolute_relative_error": m["mean_absolute_relative_error"],
                "relative_mae_ci95": m["mean_absolute_relative_error_95pct_night_bootstrap_ci"],
                "mae": m["mae"],
                "rmse": m["rmse"],
                "mean_relative_error": m["mean_relative_error"],
                "mae_ci95": m["mae_95pct_night_bootstrap_ci"],
            }
        )

    historical_delta = {
        key: (
            scores["models"][key]["mean_absolute_relative_error"]
            - HISTORICAL_FOUR_COMPONENT_MAE[hist_key]
            if key in scores["models"] and hist_key in HISTORICAL_FOUR_COMPONENT_MAE
            else None
        )
        for key, hist_key in (
            ("NSB_continuum_native", "NSB_continuum_native"),
            ("NSB_oxygen_native", "NSB_oxygen_native"),
            ("NSB_continuum_f27", "NSB_continuum_F107"),
            ("NSB_oxygen_f27", "NSB_oxygen_F107"),
            ("nsb2_f27", "nsb2_matched_F107"),
            ("baseline", "median_baseline"),
        )
    }

    admission = {
        "mode": "conditional",
        "independence_status": "DEVELOPMENT_ONLY",
        "scientific_status": (
            "Retrospective development evaluation on previously inspected Patat cohort; "
            "not an independent holdout for Oxygen default admission."
        ),
        "admitted_observations": len(observations),
        "admitted_nights": len(set(nights.tolist())),
        "evaluation_observations": int(evaluation.sum()),
        "evaluation_nights": len(set(nights[evaluation].tolist())),
        "calibration_nights": sorted(int(n) for n in calibration_nights),
        "four_components_present": True,
        "moonlight_mode": "physical_zero_under_selection",
        "observable": config["observable"]["name"],
        "wavelength_nm": [wave_min, wave_max],
        "nsb": nsb_info,
        "nsb2_commit": config["models"]["nsb2"]["commit"],
        "nsb2_version": version("nsb2"),
        "oxygen_default_admission": "NO-GO",
        "lvm_absolute_equivalence": "PENDING_EXTERNAL_EVIDENCE",
    }

    # Figures
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=(6.2, 5.5))
    ax.scatter(
        observed[evaluation],
        pred_arrays["NSB_continuum_f27"][evaluation],
        s=16,
        alpha=0.7,
        label="NSB continuum (F10.7)",
        color="#5b7c99",
    )
    ax.scatter(
        observed[evaluation],
        pred_arrays["NSB_oxygen_f27"][evaluation],
        s=16,
        alpha=0.7,
        label="NSB Oxygen (F10.7)",
        color="#c47b4a",
    )
    ax.scatter(
        observed[evaluation],
        pred_arrays["nsb2_f27"][evaluation],
        s=16,
        alpha=0.7,
        label="nsb2 (F10.7)",
        color="#2f6f4e",
    )
    hi = float(
        max(
            observed[evaluation].max(),
            pred_arrays["NSB_oxygen_f27"][evaluation].max(),
            pred_arrays["nsb2_f27"][evaluation].max(),
        )
    )
    ax.plot([0, hi * 1.05], [0, hi * 1.05], "k-", lw=1)
    ax.set_xlabel(r"Observed [ph cm$^{-2}$ ns$^{-1}$ sr$^{-1}$]")
    ax.set_ylabel("Predicted total (four-component)")
    ax.set_title("Patat 365–650 nm — full-model evaluation set")
    ax.legend(fontsize=8)
    ax.grid(True, alpha=0.3)
    fig.tight_layout()
    fig.savefig(figures / "full_obs_vs_pred.png", dpi=150)
    plt.close(fig)

    # Component mean bars
    def mean_comp(plist: list[RadiancePrediction]) -> dict[str, float]:
        keys = ("zodiacal", "airglow", "starlight", "moonlight")
        return {k: float(np.mean([p.component_radiance[k] for p in plist])) for k in keys}

    comp_means = {
        "NSB_continuum_f27": mean_comp(preds["NSB_continuum_f27"]),
        "NSB_oxygen_f27": mean_comp(preds["NSB_oxygen_f27"]),
        "nsb2_f27": mean_comp(preds["nsb2_f27"]),
    }
    fig, ax = plt.subplots(figsize=(8.0, 4.4))
    labels = list(comp_means)
    x = np.arange(len(labels))
    bottom = np.zeros(len(labels))
    colors = {
        "zodiacal": "#c99727",
        "airglow": "#55a87a",
        "starlight": "#667cb5",
        "moonlight": "#9a8dba",
    }
    for part in ("zodiacal", "airglow", "starlight", "moonlight"):
        vals = np.array([comp_means[lab][part] for lab in labels])
        ax.bar(x, vals, bottom=bottom, label=part, color=colors[part])
        bottom = bottom + vals
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=15, ha="right")
    ax.set_ylabel(r"Mean component radiance [ph cm$^{-2}$ ns$^{-1}$ sr$^{-1}$]")
    ax.set_title("Four-component means (all selected spectra, F10.7-matched)")
    ax.legend()
    ax.grid(True, axis="y", alpha=0.3)
    fig.tight_layout()
    fig.savefig(figures / "full_component_means.png", dpi=150)
    plt.close(fig)

    # Paired relative MAE bars on evaluation set
    fig, ax = plt.subplots(figsize=(8.5, 4.2))
    names = [r["configuration"] for r in primary_table]
    maes = [r["mean_absolute_relative_error"] for r in primary_table]
    yerr = []
    for r in primary_table:
        ci = r["relative_mae_ci95"]
        if ci is None:
            yerr.append((0.0, 0.0))
        else:
            yerr.append((r["mean_absolute_relative_error"] - ci[0], ci[1] - r["mean_absolute_relative_error"]))
    yerr_arr = np.array(yerr).T
    ax.barh(names, maes, xerr=yerr_arr, color="#5b7c99", alpha=0.85)
    ax.axvline(HISTORICAL_FOUR_COMPONENT_MAE["median_baseline"], color="0.3", ls="--", lw=1, label="hist. baseline")
    ax.set_xlabel("Relative MAE (evaluation nights)")
    ax.set_title("Full four-component primary scores (this run)")
    ax.invert_yaxis()
    ax.grid(True, axis="x", alpha=0.3)
    fig.tight_layout()
    fig.savefig(figures / "full_primary_mae.png", dpi=150)
    plt.close(fig)

    # Residuals vs F10.7
    f27_arr = np.array([r["observed_f107_centered_27d"] for r in observations])
    fig, ax = plt.subplots(figsize=(7.5, 4.2))
    for key, color, lab in (
        ("NSB_continuum_f27", "#5b7c99", "continuum"),
        ("NSB_oxygen_f27", "#c47b4a", "Oxygen"),
        ("nsb2_f27", "#2f6f4e", "nsb2"),
    ):
        rel = (pred_arrays[key][evaluation] - observed[evaluation]) / observed[evaluation]
        ax.scatter(f27_arr[evaluation], rel, s=14, alpha=0.7, color=color, label=lab)
    ax.axhline(0, color="k", lw=0.8)
    ax.set_xlabel("Centered 27-day observed F10.7 [sfu]")
    ax.set_ylabel("Relative residual (pred−obs)/obs")
    ax.set_title("Evaluation residuals vs measured solar activity")
    ax.legend()
    ax.grid(True, alpha=0.3)
    fig.tight_layout()
    fig.savefig(figures / "full_residual_f107.png", dpi=150)
    plt.close(fig)

    rows_out = []
    for i, _spectrum in enumerate(spectra):
        row = {
            **observations[i],
            "split": "calibration" if train[i] else "evaluation",
            "baseline": baseline_value,
        }
        for name, plist in preds.items():
            row[name] = plist[i].to_dict()
        rows_out.append(row)

    metrics = {
        "schema": "nsb-ctao-four-component-v1",
        "observable": config["observable"],
        "selection_summary": selection_summary,
        "f_band": f_band,
        "baseline_radiance": baseline_value,
        "scores": scores,
        "primary_table": primary_table,
        "component_means_f27": comp_means,
        "historical_reference_mae": HISTORICAL_FOUR_COMPONENT_MAE,
        "historical_mae_delta_this_minus_reference": historical_delta,
        "admission": admission,
        "paired_focus": {
            "NSB_oxygen_f27_minus_nsb2_f27": _paired(scores, "NSB_oxygen_f27", "nsb2_f27"),
            "NSB_oxygen_f27_minus_NSB_continuum_f27": _paired(
                scores, "NSB_oxygen_f27", "NSB_continuum_f27"
            ),
            "NSB_oxygen_f27_minus_baseline": _paired(scores, "NSB_oxygen_f27", "baseline"),
        },
    }
    manifest = {
        "campaign": "ctao-four-component-v02",
        "created_utc": datetime.now(tz=UTC).isoformat(),
        "config_sha256": hashlib.sha256(config_path.read_bytes()).hexdigest(),
        "solar_sha256": hashlib.sha256(solar_path.read_bytes()).hexdigest(),
        "dataset": acquisition,
        "nsb": nsb_info,
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
        "nsb_runtime.json": nsb_info,
    }
    write_artifacts(output_dir, payloads, figures, manifest)
    return {
        "output": str(output_dir),
        "admission": admission,
        "primary_table": primary_table,
        "scores": scores,
        "metrics": metrics,
    }
