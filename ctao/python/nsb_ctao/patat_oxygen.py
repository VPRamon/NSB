"""Patat/FORS1 continuum vs Oxygen vs nsb2 comparison for the CTAO notebook.

The Patat cohort informed Oxygen development. Results here are labeled
**development diagnostics**, not independent holdout validation.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import nsb
import numpy as np
import siderust
from astropy.time import Time

from ..datasets.patat import acquire, extract_library, load_selected
from ..evaluation.patat_spectral import observed_photon_radiance


@dataclass(frozen=True)
class PatatModelRow:
    observation_id: str
    civil_night: str
    observed: float
    continuum: float
    oxygen: float
    nsb2: float | None
    f107_header_sfu: float
    airmass: float


def _relative_abs_error(pred: float, obs: float) -> float:
    return abs(pred - obs) / obs


def _night_cluster_bootstrap_mae(
    nights: list[str],
    abs_rel: np.ndarray,
    *,
    seed: int,
    repetitions: int = 2000,
) -> dict[str, float]:
    """Bootstrap mean absolute relative error clustering by civil night."""
    rng = np.random.default_rng(seed)
    by_night: dict[str, list[int]] = {}
    for i, night in enumerate(nights):
        by_night.setdefault(night, []).append(i)
    night_keys = list(by_night)
    samples = []
    for _ in range(repetitions):
        chosen = rng.choice(night_keys, size=len(night_keys), replace=True)
        idx = [j for n in chosen for j in by_night[n]]
        samples.append(float(np.mean(abs_rel[idx])))
    arr = np.asarray(samples)
    return {
        "point": float(np.mean(abs_rel)),
        "ci95_low": float(np.quantile(arr, 0.025)),
        "ci95_high": float(np.quantile(arr, 0.975)),
    }


def run_patat_oxygen_comparison(
    *,
    config_path: Path,
    data_dir: Path,
    output_dir: Path,
    f107_sfu: float | None = 130.0,
    max_spectra: int | None = None,
    include_nsb2: bool = True,
    seed: int = 20260324,
) -> dict[str, Any]:
    """Execute continuum/Oxygen(/nsb2) on the registered Patat selection."""
    config = json.loads(config_path.read_text())
    output_dir.mkdir(parents=True, exist_ok=True)
    archive = data_dir / config["dataset"]["filename"]
    acquisition = acquire(archive, config["dataset"]["sha256"])
    library = extract_library(archive, data_dir / "library")
    selection = {
        **config["selection"],
        "wavelength_min_nm": config["observable"]["wavelength_min_nm"],
        "wavelength_max_nm": config["observable"]["wavelength_max_nm"],
    }
    spectra, selection_summary = load_selected(library, selection)
    if max_spectra is not None:
        spectra = spectra[: max_spectra]
    if not spectra:
        raise RuntimeError("No Patat spectra admitted")

    site = config["dataset"]["site"]
    observer = siderust.Observer(
        lon_deg=site["longitude_deg"],
        lat_deg=site["latitude_deg"],
        height_m=site["height_m"],
    )
    wave_min = float(config["observable"]["wavelength_min_nm"])
    wave_max = float(config["observable"]["wavelength_max_nm"])
    cal = float(config.get("calibration_fractional_sigma", 0.05))

    nsb2_adapter = None
    nsb2_error = None
    if include_nsb2:
        try:
            from ..evaluation.patat_spectral import Nsb2FourComponentAdapter

            nsb2_adapter = Nsb2FourComponentAdapter(config["models"]["nsb2"]["commit"], site)
        except Exception as exc:
            nsb2_error = str(exc)

    models = {
        "continuum": nsb.AirglowModel.PARANAL_PALACE_V1,
        "oxygen": nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1,
    }
    # Full four-component totals for fair nsb2 comparison require starlight scaling;
    # for airglow-focused Oxygen experiment we report airglow-only and total-where-possible.
    rows: list[PatatModelRow] = []
    for spectrum in spectra:
        measured = observed_photon_radiance(
            spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max, calibration_fractional_sigma=cal
        )
        instant = Time(spectrum.mjd_obs, format="mjd", scale="utc").to_datetime(timezone=UTC)
        target = siderust.Direction(ra_deg=spectrum.ra_deg, dec_deg=spectrum.dec_deg)
        query = nsb.PointQuery(
            observer, instant, target, components=nsb.ComponentMask.AIRGLOW
        )
        preds: dict[str, float] = {}
        for label, model in models.items():
            base = nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(model)
            solar = spectrum.sun_flux_sfu if f107_sfu is None else f107_sfu
            if solar is not None:
                base = base.with_solar_radio_flux_sfu(float(solar))
            evaluator = nsb.NsbEvaluator(base)
            preds[label] = float(evaluator.integrate_band(query, wave_min, wave_max))
        nsb2_val = None
        if nsb2_adapter is not None:
            pred = nsb2_adapter.predict(spectrum, wave_min_nm=wave_min, wave_max_nm=wave_max)
            # Prefer airglow component for apples-to-apples airglow-only NSB comparison.
            nsb2_val = float(
                pred.component_radiance.get("airglow", pred.predicted_radiance)
            )
        rows.append(
            PatatModelRow(
                observation_id=spectrum.observation_id,
                civil_night=spectrum.civil_night,
                observed=float(measured["radiance"]),
                continuum=preds["continuum"],
                oxygen=preds["oxygen"],
                nsb2=nsb2_val,
                f107_header_sfu=float(spectrum.sun_flux_sfu),
                airmass=float(spectrum.airmass),
            )
        )

    nights = [r.civil_night for r in rows]
    obs = np.array([r.observed for r in rows])
    scores: dict[str, Any] = {}
    for name, pred in (
        ("NSB_continuum_airglow", np.array([r.continuum for r in rows])),
        ("NSB_oxygen_airglow", np.array([r.oxygen for r in rows])),
    ):
        abs_rel = np.abs(pred - obs) / obs
        signed = (pred - obs) / obs
        scores[name] = {
            "n": len(rows),
            "bias_relative": float(np.mean(signed)),
            "mae_relative": float(np.mean(abs_rel)),
            "rmse_relative": float(np.sqrt(np.mean(signed**2))),
            "mae_relative_night_bootstrap": _night_cluster_bootstrap_mae(
                nights, abs_rel, seed=seed
            ),
            "note": (
                "Airglow-only NSB prediction vs full observed photon radiance — "
                "not identical to the historical four-component Patat operator."
            ),
        }
    if any(r.nsb2 is not None for r in rows):
        pred = np.array([r.nsb2 for r in rows], dtype=float)
        abs_rel = np.abs(pred - obs) / obs
        signed = (pred - obs) / obs
        scores["nsb2_airglow_or_total"] = {
            "n": len(rows),
            "bias_relative": float(np.mean(signed)),
            "mae_relative": float(np.mean(abs_rel)),
            "rmse_relative": float(np.sqrt(np.mean(signed**2))),
            "mae_relative_night_bootstrap": _night_cluster_bootstrap_mae(
                nights, abs_rel, seed=seed + 1
            ),
            "note": "Uses nsb2 adapter airglow component when available.",
        }
        # Paired oxygen vs nsb2
        oxy = np.array([r.oxygen for r in rows])
        paired = np.abs(oxy - obs) / obs - abs_rel
        scores["paired_oxygen_minus_nsb2_abs_rel"] = {
            "mean": float(np.mean(paired)),
            "night_bootstrap": _night_cluster_bootstrap_mae(
                nights, paired, seed=seed + 2
            ),
        }

    payload = {
        "schema": "nsb-ctao-patat-oxygen-v1",
        "independence_status": "DEVELOPMENT_ONLY — Patat cohort informed Oxygen development",
        "admission_status": "not_independent_holdout",
        "observable": config["observable"],
        "selection_summary": selection_summary,
        "acquisition": acquisition,
        "n_spectra": len(rows),
        "n_nights": len(set(nights)),
        "f107_mode": "fixed" if f107_sfu is not None else "header",
        "f107_sfu": f107_sfu,
        "nsb_version": getattr(nsb, "__version__", None),
        "nsb2_available": nsb2_adapter is not None,
        "nsb2_error": nsb2_error,
        "scores": scores,
        "rows": [asdict(r) for r in rows],
        "historical_reference_four_component_mae": {
            "note": "Frozen four-component Patat operator (not regenerated by this airglow-only path)",
            "NSB_continuum_native": 0.36103,
            "NSB_oxygen_native": 0.23882,
            "NSB_continuum_F107": 0.35027,
            "NSB_oxygen_F107": 0.16573,
            "nsb2_matched_F107": 0.17425,
            "median_baseline": 0.15245,
        },
        "executed_at_utc": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    (output_dir / "patat_oxygen_metrics.json").write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return payload
