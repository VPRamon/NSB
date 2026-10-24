# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Notebook stages and the explicit manual campaign command."""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np

from . import data
from .models import COMPONENTS, MODELS, Models, external_cache_inventory
from .report import HISTORICAL, finish, new_run
from .runtime import verify_nsb2, verify_runtime
from .statistics import align, baseline, scores


@dataclass
class Config:
    mode: str = "offline"  # offline synthetic, historical display, or current real-data execution
    cache: str = str(Path.home() / ".cache" / "nsb-observational")
    output: str = "validation-results"
    offline_data: bool = False
    runtime_mode: str = "source"
    nsb_commit: str = "f5e1d22689ed417c0dcf9df72da54365c30f5bd5"
    nsb_version: str | None = None
    receipt: str = ""
    solar_file: str | None = None
    repetitions: int = 2000
    seed: int = 182


def synthetic():
    """Small deterministic pipeline fixture; NEVER model/observational evidence."""
    rows = []
    predictions = {s: {m: [] for m in MODELS} for s in ("native", "matched")}
    wave = np.arange(3600.0, 6601.0, 5.0)
    for night in range(8):
        for exposure in range(3):
            i = len(rows)
            energy = 1.1e-17 * (1 + night * 0.05) * (1 + 0.1 * np.sin(wave / 100)) + 1e-16 * np.exp(
                -(((wave - 5577.34) / 10) ** 2)
            )
            row = dict(
                id=f"SYNTHETIC-{i:03}",
                night=60000 + night,
                split="calibration" if night < 2 else "evaluation",
                observed=data.measured_radiance(wave, energy),
                mjd=60000.0 + night + exposure * 0.01,
                ra=90.0,
                dec=-30.0,
                airmass=1.2,
                header_sfu=130.0,
                f27=90.0 + night * 20,
                wave=wave,
                energy=energy,
            )
            rows.append(row)
            for scenario in predictions:
                for j, model in enumerate(MODELS):
                    factor = (0.65, 0.85, 0.90)[j] + 0.09 * np.sin(i + j)
                    total = row["observed"] * factor
                    parts = dict(
                        zip(COMPONENTS, (total * 0.35, total * 0.5, total * 0.15, 0.0), strict=True)
                    )
                    predictions[scenario][model].append(
                        dict(id=row["id"], total=total, components=parts)
                    )
    bins = np.arange(550.0, 645.0)
    spectrum = dict(
        id=rows[0]["id"],
        air_wavelength_nm=(wave / 10).tolist(),
        observed_total_density=data.photon_density(wave, rows[0]["energy"]).tolist(),
        vacuum_bin_edges_nm=bins.tolist(),
        standard=[0.0001] * 94,
        oxygen=[0.00015] * 94,
        scenario="SYNTHETIC fixture; not physical predictions",
    )
    return rows, predictions, spectrum


def prepare(config):
    if config.mode not in ("offline", "historical", "current"):
        raise ValueError("mode must be offline, historical, or current")
    state = dict(
        config=config,
        rows=[],
        predictions={},
        spectral=None,
        metrics={},
        provenance={},
        availability={},
        failures=[],
    )
    if config.mode == "historical":
        state["availability"]["historical"] = HISTORICAL["classification"]
        return state
    if config.mode == "offline":
        state["rows"], state["predictions"], state["spectral"] = synthetic()
        state["availability"] = {"native": "synthetic fixture", "matched": "synthetic fixture"}
        return state
    # Refuse arbitrary preinstalled NSB before observations are evaluated.
    state["provenance"]["nsb"] = verify_runtime(
        config.receipt,
        mode=config.runtime_mode,
        commit=config.nsb_commit,
        package_version=config.nsb_version,
    )
    state["provenance"]["nsb2"] = verify_nsb2()
    from nsb2 import ASSETS_PATH

    skycalc = ASSETS_PATH / "eso_skycalc_airglow_130sfu.dat"
    if data.sha256(skycalc) != "f3ac8d8260d3eafcf79ed80dc9cf2a04ecd852d30176ec44888764b491c4841e":
        raise ValueError("nsb2 SkyCalc source checksum differs from frozen protocol")
    state["provenance"]["nsb2"]["skycalc_sha256"] = data.sha256(skycalc)
    from astropy.utils import data as astropy_data

    astropy_data.conf.allow_internet = not config.offline_data
    cache = Path(config.cache).expanduser().resolve()
    repository = Path(__file__).resolve().parents[4]
    if cache == repository or repository in cache.parents:
        raise ValueError("Store downloaded data outside the repository")
    archive = data.acquire(
        cache / "skyspectra.tar", data.PATAT_SHA, data.PATAT_URLS, offline=config.offline_data
    )
    library, inventory = data.extract(archive, cache / "library")
    print(
        f"Verified archive and {len(inventory)} FITS products; selecting frozen cohort", flush=True
    )
    rows, selection = data.load_cohort(library)
    state["rows"] = rows
    state["provenance"].update(
        dataset={
            "sha256": data.sha256(archive),
            "bytes": archive.stat().st_size,
            "urls": data.PATAT_URLS,
            "fits_inventory": inventory,
        },
        selection=selection,
    )
    state["availability"]["native"] = "available"
    try:
        solar = (
            Path(config.solar_file).expanduser()
            if config.solar_file
            else data.acquire(
                cache / "solar-observed.txt",
                data.SOLAR_SHA,
                (data.SOLAR_URL,),
                offline=config.offline_data,
            )
        )
        f27 = data.solar_windows(solar, rows)
        for row in rows:
            row["f27"] = f27[row["id"]]
        state["provenance"]["solar"] = {
            "input_sha256": data.sha256(solar),
            "source_sha256": data.SOLAR_SHA,
            "url": data.SOLAR_URL,
            "convention": "NRC observed noontime; centered 27 complete UTC days; not adjusted",
            "outside_palace_training": sum(not 67 <= r["f27"] <= 166 for r in rows),
        }
        state["availability"]["matched"] = "available"
    except (OSError, ValueError, RuntimeError) as error:
        state["availability"]["matched"] = f"UNAVAILABLE: {error}"
        print(f"Matched measured solar scenario unavailable: {error}", flush=True)
    return state


def evaluate(state):
    if state["config"].mode != "current":
        return state
    models = Models()
    state["provenance"]["starlight_band_fraction"] = models.fraction
    for scenario, status in state["availability"].items():
        if status != "available":
            continue
        state["predictions"][scenario] = {m: [] for m in MODELS}
        for i, row in enumerate(state["rows"]):
            print(f"{scenario}: {i + 1}/{len(state['rows'])} {row['id']}", flush=True)
            for model in MODELS:
                try:
                    prediction = models.predict(
                        row, model, row["f27"] if scenario == "matched" else None
                    )
                    state["predictions"][scenario][model].append(prediction)
                except Exception as error:
                    state["failures"].append(
                        dict(scenario=scenario, model=model, id=row["id"], reason=str(error))
                    )
    first = state["rows"][0]
    state["spectral"] = models.spectral_example(first, first.get("f27"))
    state["provenance"]["astropy_input_cache"] = external_cache_inventory()
    return state


def analyze(state):
    if state["config"].mode == "historical":
        return state
    rows = state["rows"]
    base = baseline(rows)
    selection = [i for i, row in enumerate(rows) if row["split"] == "evaluation"]
    for scenario, predictions in state["predictions"].items():
        try:
            vectors = align(rows, predictions)
        except ValueError as error:
            state["availability"][scenario] = f"INVALID: {error}; no intersection scoring"
            continue
        vectors["baseline"] = np.full(len(rows), base)
        state["metrics"][scenario] = scores(
            [rows[i]["observed"] for i in selection],
            {key: v[selection] for key, v in vectors.items()},
            [rows[i]["night"] for i in selection],
            state["config"].repetitions,
            state["config"].seed,
        )
    state["provenance"]["baseline"] = {
        "value": base,
        "fit": "median on calibration nights only",
        "calibration_nights": sorted({r["night"] for r in rows if r["split"] == "calibration"}),
    }
    return state


def export(state):
    config = state["config"]
    complete = (
        config.mode == "current"
        and set(state["metrics"]) == {"native", "matched"}
        and not state["failures"]
    )
    classification = {
        "offline": "SYNTHETIC OFFLINE DEMONSTRATION — no empirical validation",
        "historical": "HISTORICAL EVIDENCE DISPLAY — no executable replay",
        "current": "NEW DEVELOPMENT EXPERIMENT — no independent accuracy admission",
    }[config.mode]
    inference = {}
    threshold = 0.10  # Frozen benchmark practical relative-MAE difference.
    for scenario, result in state["metrics"].items():
        inference[scenario] = {}
        for pair, value in result["paired"].items():
            lo, hi = value["relative_mae_ci95"]
            decision = (
                "no_demonstrated_difference"
                if lo <= 0 <= hi
                else "left_has_practically_lower_error"
                if hi < -threshold
                else "right_has_practically_lower_error"
                if lo > threshold
                else "difference_not_practically_meaningful"
            )
            inference[scenario][pair] = decision
    admission = dict(
        classification=classification,
        paired_development_inference=inference,
        practical_relative_mae_threshold=threshold,
        experiment_complete=complete,
        independent_validation=False,
        oxygen="experimental opt-in; default unchanged",
        absolute_accuracy_admitted=False,
        availability=state["availability"],
        rejected_predictions=state["failures"],
        missing_predictions={
            s: {m: len(state["rows"]) - len(v) for m, v in p.items()}
            for s, p in state["predictions"].items()
        },
    )
    path = new_run(config.output)
    return finish(
        path,
        config=asdict(config),
        rows=state["rows"],
        predictions=state["predictions"],
        metrics=state["metrics"],
        admission=admission,
        provenance=state["provenance"],
        spectral=state["spectral"],
    )


def run(config):
    state = prepare(config)
    evaluate(state)
    analyze(state)
    return export(state)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("offline", "historical", "current"), default="offline")
    parser.add_argument("--config", type=Path)
    args = parser.parse_args()
    config = (
        Config(**json.loads(args.config.read_text())) if args.config else Config(mode=args.mode)
    )
    path = run(config)
    print(f"Report: {path.resolve()}/report.html")
    if config.mode == "current":
        admission = json.loads((path / "admission.json").read_text())
        if not admission["experiment_complete"]:
            raise SystemExit("Incomplete experiment: inspect the exported admission report")


if __name__ == "__main__":
    main()
