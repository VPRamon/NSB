#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Reproduce the registered F10.7 cadence comparison for NSB issue #295.

Runs offline from checksum-pinned inputs. The notebook is the authoritative
source of the registered cohort, NSB integration and night-clustered scorer.
No downloaded inputs, new physical coefficients or NSB defaults are introduced.
"""
from __future__ import annotations

import argparse
import ast
from datetime import UTC, date, timedelta
import hashlib
from io import BytesIO
from itertools import combinations
import json
import importlib.metadata as metadata
from importlib.machinery import EXTENSION_SUFFIXES
import platform
import subprocess
from pathlib import Path
import tarfile

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
NOTEBOOK = ROOT / "examples/observational-validation.ipynb"
EVIDENCE = ROOT / "docs/lessons-learned/artifacts/solar-activity-f107-cadence-evidence.json"
NAMES = (
    "operational_automatic",
    "trailing27_latency1",
    "previous_month_mean",
    "latest_daily_latency1",
    "centered27_diagnostic",
    "same_month_finalized_diagnostic",
)
DEFINITIONS = {
    "measured_radiance", "exclusion", "load_cohort", "Models", "reference_integral",
    "errors", "nightly_errors", "scores", "condition_scores",
}
CONSTANTS = {
    "SITE", "COMPONENTS", "PATAT_SHA", "SOLAR_SHA", "COHORT_SHA",
    "CALIBRATION_NIGHTS", "HC_ERG_CM", "ARCSEC2_PER_SR",
}


def verify_sha256(path: Path, expected: str) -> None:
    """Require local pinned assets; never fetch data during evaluation."""
    if not path.is_file():
        raise FileNotFoundError(f"Input missing: {path}")
    with path.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != expected:
        raise ValueError(f"{path}: expected sha256 {expected}, got {actual}")


def solar_daily(path: Path) -> dict[date, float]:
    """Read the same observed, not adjusted, NOAA-mirrored listing as the notebook."""
    daily: dict[date, float] = {}
    seen: set[date] = set()
    for line in path.read_text().splitlines():
        fields = line.split()
        if not fields:
            continue
        key = fields[0]
        if len(key) != 8 or not key.isdigit() or len(fields) > 2:
            raise ValueError(f"Malformed solar observation: {line!r}")
        day = date(int(key[:4]), int(key[4:6]), int(key[6:8]))
        if day in seen:
            raise ValueError(f"Duplicate observed day {day}")
        seen.add(day)
        # The pinned listing represents missing observations by a date alone.
        if len(fields) == 1 or fields[1] == ".":
            continue
        value = float(fields[1])
        if not np.isfinite(value) or value <= 0:
            raise ValueError(f"Invalid observed F10.7 for {day}")
        daily[day] = value
    if not daily:
        raise ValueError("No usable daily observed solar data")
    return daily


def complete_mean(daily: dict[date, float], first: date, last: date) -> float:
    """Require all daily samples, with inclusive endpoints."""
    if first > last:
        raise ValueError("Reversed solar interval")
    values = []
    day = first
    while day <= last:
        if day not in daily:
            raise ValueError(f"Incomplete solar interval: missing {day}")
        value = daily[day]
        if not np.isfinite(value) or value <= 0:
            raise ValueError(f"Invalid observed F10.7 for {day}")
        values.append(value)
        day += timedelta(days=1)
    return float(np.mean(values))


def candidate_flux(daily: dict[date, float], day: date) -> dict[str, float | None]:
    """Simulated calendar-day cutoffs, NOT audited historical publication times."""
    month_start = day.replace(day=1)
    previous_end = month_start - timedelta(days=1)
    previous_start = previous_end.replace(day=1)
    next_start = (month_start.replace(year=month_start.year + 1, month=1)
                  if month_start.month == 12 else month_start.replace(month=month_start.month + 1))
    return {
        "operational_automatic": None,
        "trailing27_latency1": complete_mean(daily, day - timedelta(days=27),
                                              day - timedelta(days=1)),
        "previous_month_mean": complete_mean(daily, previous_start, previous_end),
        "latest_daily_latency1": complete_mean(
            daily, day - timedelta(days=1), day - timedelta(days=1)),
        "centered27_diagnostic": complete_mean(daily, day - timedelta(days=13),
                                               day + timedelta(days=13)),
        "same_month_finalized_diagnostic": complete_mean(
            daily, month_start, next_start - timedelta(days=1)),
    }


def notebook_namespace() -> dict:
    """Reuse frozen notebook definitions, rather than duplicate selection/scoring."""
    import nsb
    import siderust
    from astropy.utils import data as astropy_data, iers
    astropy_data.conf.allow_internet = False
    iers.conf.auto_download = False
    import astropy.units as u
    from astropy.io import fits
    from astropy.time import Time
    from scipy.integrate import simpson

    nb = json.loads(NOTEBOOK.read_text())
    selected = []
    found = set()
    for cell in nb["cells"]:
        if cell["cell_type"] != "code":
            continue
        for node in ast.parse("".join(cell["source"])).body:
            if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in DEFINITIONS:
                if node.name in found:
                    raise ValueError(f"Duplicate notebook definition: {node.name}")
                selected.append(node)
                found.add(node.name)
            elif isinstance(node, ast.Assign) and len(node.targets) == 1:
                target = node.targets[0]
                if isinstance(target, ast.Name) and target.id in CONSTANTS:
                    if target.id in found:
                        raise ValueError(f"Duplicate notebook constant: {target.id}")
                    selected.append(node)
                    found.add(target.id)
    missing = (DEFINITIONS | CONSTANTS) - found
    if missing:
        raise ValueError(f"Notebook registration changed: missing {sorted(missing)}")
    scope = {
        "np": np, "nsb": nsb, "siderust": siderust, "u": u, "fits": fits,
        "Time": Time, "simpson": simpson, "UTC": UTC, "date": date,
        "timedelta": timedelta, "tarfile": tarfile, "BytesIO": BytesIO,
        "hashlib": hashlib, "combinations": combinations,
    }
    exec(compile(ast.Module(body=selected, type_ignores=[]), str(NOTEBOOK), "exec"), scope)
    return scope


def candidate_minus_automatic(group: dict, candidate: str) -> dict:
    """Convert Automatic-minus-candidate bootstrap intervals to candidate-minus-Automatic."""
    pair = group["paired"][f"operational_automatic_minus_{candidate}"]
    lo, hi = pair["relative_mae_ci95"]
    return {
        "difference": -pair["relative_mae_difference"],
        "ci95": [-hi, -lo],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--patat", type=Path, required=True, help="Registered Patat FITS tar archive")
    parser.add_argument("--solar", type=Path, required=True, help="Observed daily Penticton listing")
    parser.add_argument("--rieke", type=Path, required=True, help="STScI solar_spec.fits")
    parser.add_argument("--output", type=Path, required=True, help="Write regenerated JSON here")
    args = parser.parse_args()

    original = json.loads(EVIDENCE.read_text())
    known = original["inputs"]
    verify_sha256(args.patat, known["patat_sha256"])
    verify_sha256(args.solar, known["solar_sha256"])
    verify_sha256(args.rieke, known["rieke_sha256"])
    ns = notebook_namespace()
    for field, notebook_name in (("patat_sha256", "PATAT_SHA"),
                                 ("solar_sha256", "SOLAR_SHA"),
                                 ("cohort_sha256", "COHORT_SHA")):
        if known[field] != ns[notebook_name]:
            raise ValueError(f"Pinned {field} disagrees with the registered notebook")

    from astropy.time import Time
    rows, selection = ns["load_cohort"](args.patat)
    if selection["identity_sha256"] != known["cohort_sha256"]:
        raise ValueError("Cohort fingerprint changed")
    eval_ids = np.array([i for i, row in enumerate(rows) if row["split"] == "evaluation"])
    nights = np.array([rows[i]["night"] for i in eval_ids])
    if (len(rows), len(set(row["night"] for row in rows)),
        len(eval_ids), len(set(nights))) != (103, 22, 94, 17):
        raise ValueError("Registered 103/22 + 94/17 partition changed")

    model = ns["Models"].__new__(ns["Models"])
    model.nsb = ns["nsb"]
    model.siderust = ns["siderust"]
    model.observer = ns["siderust"].Observer(
        lon_deg=ns["SITE"]["longitude"],
        lat_deg=ns["SITE"]["latitude"],
        height_m=ns["SITE"]["height"],
    )
    # Recompute the registered source-grid photon integral from verified bytes.
    from astropy.constants import h, c
    u = ns["u"]
    with ns["fits"].open(args.rieke) as hdus:
        wave = hdus[1].data["WAVELENGTH"] * u.angstrom
        energy = hdus[1].data["FLUX"] * u.erg / u.s / u.cm**2 / u.angstrom
    photon = (energy * wave / (h * c)).to_value(1 / u.s / u.cm**2 / u.nm)
    wave = wave.to_value(u.nm)
    integral = ns["reference_integral"]
    model.fraction = integral(wave, photon, 365, 650) / integral(wave, photon, 300, 650)
    if not np.isclose(model.fraction, known["rieke_fraction_365_650_over_300_650"],
                      rtol=0, atol=1e-14):
        raise ValueError("Registered Rieke band fraction changed")
    daily = solar_daily(args.solar)
    solar_inputs: dict[str, list[float | None]] = {name: [] for name in NAMES}
    oxygen: dict[str, list[float]] = {name: [] for name in NAMES}
    continuum: dict[str, list[float]] = {name: [] for name in NAMES}
    automatic_provenance = set()
    for row in rows:
        day = Time(row["mjd"], format="mjd").to_datetime().date()
        fluxes = candidate_flux(daily, day)
        fixed_components = {}
        for name in NAMES:
            sfu = fluxes[name]
            solar_inputs[name].append(sfu)
            for label, predictions in (("NSB Automatic", oxygen), ("NSB continuum", continuum)):
                prediction = model.predict(row, label, sfu)
                predictions[name].append(prediction["total"])
                fixed = {k: v for k, v in prediction["components"].items() if k != "airglow"}
                if label in fixed_components and fixed != fixed_components[label]:
                    raise ValueError("Non-airglow components changed between solar scenarios")
                fixed_components[label] = fixed
                if sfu is None:
                    explicit = model.predict(row, label, known["climatology_sfu"])
                    if prediction["components"] != explicit["components"]:
                        raise ValueError("Automatic differs from registered climatology")
        # Inspect actual API metadata as well as numerical equivalence.
        ev = ns["nsb"].NsbEvaluator(ns["nsb"].NsbModelConfig.generic_clear_sky())
        component = ev.evaluate(model.query(row, ns["nsb"].ComponentMask.AIRGLOW)).components[0]
        provenance = component.metadata.solar_activity_provenance
        if "kind=climatology" not in (provenance or ""):
            raise ValueError("Automatic no longer resolves to climatology")
        automatic_provenance.add(provenance)

    y = np.array([rows[i]["observed"] for i in eval_ids])
    spectrum_oxygen = {n: np.array(v)[eval_ids] for n, v in oxygen.items()}
    spectrum_continuum = {n: np.array(v)[eval_ids] for n, v in continuum.items()}
    f27 = np.array(solar_inputs["centered27_diagnostic"], float)[eval_ids]
    mask = (f27 >= 67) & (f27 <= 166)
    if (int(mask.sum()), len(set(nights[mask])), int((~mask).sum())) != (21, 8, 73):
        raise ValueError("Registered F27 training-domain membership changed")

    full = ns["scores"](y, spectrum_oxygen, nights, 2000, 182)
    within = ns["condition_scores"](y, spectrum_oxygen, nights, mask, 2000, 182)
    metrics = {}
    for name in NAMES:
        metric = full["models"][name]
        continuum_metric = ns["errors"](y, spectrum_continuum[name])
        metrics[name] = {
            "relative_mae": metric["mean_absolute_relative_error"],
            "relative_mae_ci95": metric["relative_mae_ci95"],
            "bias_ci95": metric["bias_ci95"],
            "mae": metric["mae"],
            "night_weighted_mae": metric["night_weighted_mae"],
            "mean_signed_error": metric["mean_signed_error"],
            "continuum_mae": continuum_metric["mae"],
            "continuum_relative_mae": continuum_metric["mean_absolute_relative_error"],
        }
        if name in original["primary_oxygen_metrics"]:
            reference = original["primary_oxygen_metrics"][name]
            for field in ("relative_mae", "mae", "night_weighted_mae",
                          "mean_signed_error", "continuum_mae", "continuum_relative_mae"):
                if not np.isclose(metrics[name][field], reference[field], rtol=0, atol=1e-6):
                    raise ValueError(
                        f"Mismatch vs archived {name}.{field}: "
                        f"{metrics[name][field]:.9g} != {reference[field]:.9g}"
                    )

    conditions = {
        "F27 67–166 sfu (within training)": mask,
        "F27 <67 sfu (outside training)": f27 < 67,
        "F27 >166 sfu (outside training)": f27 > 166,
        "Airmass [1, 1.25]": np.array([rows[i]["airmass"] for i in eval_ids]) <= 1.25,
        "Airmass (1.25, 1.5]": np.array([rows[i]["airmass"] for i in eval_ids]) > 1.25,
    }
    conditional = {}
    for condition, condition_mask in conditions.items():
        group = ns["condition_scores"](y, spectrum_oxygen, nights, condition_mask, 2000, 182)
        conditional[condition] = {key: group[key] for key in ("n", "nights", "coverage", "models", "paired")}
        for name, archived in original["conditional_oxygen_relative_mae"].items():
            reference = archived[condition]
            if (reference["n"], reference["nights"]) != (group["n"], group["nights"]):
                raise ValueError(f"Conditional membership changed: {condition}")
            if group["n"] and not np.isclose(
                group["models"][name]["mean_absolute_relative_error"],
                reference["relative_mae"], rtol=0, atol=1e-6):
                raise ValueError(f"Conditional metric changed: {condition}/{name}")

    within_pairs = {
        name: {
            **candidate_minus_automatic(within, name),
            "n": int(mask.sum()),
            "nights": len(set(nights[mask])),
        }
        for name in NAMES if name != "operational_automatic"
    }
    result = {
        "schema_version": 1,
        "reproduction_status": "verified",
        "code_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_sha256": {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                          for path in (Path(__file__).resolve(), NOTEBOOK, ROOT / "Cargo.lock")},
        "environment": {"python": platform.python_version(), "platform": platform.platform(),
                        "packages": {name: metadata.version(name) for name in
                                     ("nsb-py", "numpy", "scipy", "astropy", "siderust", "tempoch")},
                        "nsb_extension_sha256": hashlib.sha256(
                            next(path for path in Path(ns["nsb"].__file__).parent.iterdir()
                                 if any(path.name.endswith(suffix) for suffix in EXTENSION_SUFFIXES)).read_bytes()).hexdigest()},
        "bootstrap_method": "shared whole-night draws with replacement; pooled spectrum-weighted relative MAE; percentile 2.5/97.5 (NumPy linear); candidate minus Automatic",
        "units": "photons cm^-2 ns^-1 sr^-1; relative metrics are fractions",
        "automatic_provenance": sorted(automatic_provenance),
        "rieke_fraction_365_650_over_300_650": model.fraction,
        "within_training_domain_metrics": within["models"],
        "conditional_metrics": conditional,
        "issue": 295,
        "input_sha256": {
            "patat": known["patat_sha256"], "solar": known["solar_sha256"],
            "cohort": known["cohort_sha256"], "rieke": known["rieke_sha256"],
        },
        "source_notebook": str(NOTEBOOK.relative_to(ROOT)),
        "cohort_status": "development; not independent holdout",
        "temporal_availability": "lag-one-day calendar assumption only; historical publication times not verified",
        "seed": 182,
        "bootstrap_draws": 2000,
        "evaluation_spectra": len(eval_ids),
        "evaluation_nights": len(set(nights)),
        "within_training_domain_paired": within_pairs,
        "full_cohort_paired": {
            name: candidate_minus_automatic(full, name)
            for name in NAMES if name != "operational_automatic"
        },
        "primary_oxygen_metrics": metrics,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n")
    print(f"Verified published primary metrics; wrote {args.output}")
    for name, pair in within_pairs.items():
        print(f"{name}: paired relMAE={pair['difference'] * 100:+.2f} pp "
              f"CI95=[{pair['ci95'][0] * 100:+.2f}, {pair['ci95'][1] * 100:+.2f}] pp")


if __name__ == "__main__":
    main()
