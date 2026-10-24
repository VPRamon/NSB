# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Optional factual checks against externally supplied, checksum-pinned evidence.

No private backend is imported or distributed. A newer source may differ
numerically; this comparison never promotes it to the historical executable.
"""

import argparse
import json
from pathlib import Path

import numpy as np

from .data import sha256
from .report import json_write, verify
from .statistics import scores

PINS = {
    "benchmark": "83e22386ed166dbc49d9428d064f8816a968e69d935792cb0d96ec7366336532",
    "diagnostic": "b99be79333a54ca000f628fac1924202e30c20f76764752606ce52837b37b920",
    "diagnostic_metrics": "1fcb2856f48cebd071dbec1aa6dc610078e9eb6c18cc2b14b8ccacbf81254297",
}


def load(path, name):
    if sha256(path) != PINS[name]:
        raise ValueError(f"Unregistered {name} artifact")
    return json.loads(Path(path).read_text())


def compare(run, benchmark, diagnostic=None, diagnostic_metrics=None):
    manifest = verify(run)
    rows = json.loads((run / "observations.json").read_text())
    predictions = json.loads((run / "predictions.json").read_text())
    ref = load(benchmark, "benchmark")
    historical = {r["observation_id"]: r for r in ref}
    if {r["id"] for r in rows} != set(historical) or len(rows) != len(ref):
        raise ValueError("Observation identities differ")
    for r in rows:
        old = historical[r["id"]]
        if (r["night"], r["split"]) != (old["civil_night"], old["split"]):
            raise ValueError("Night partition differs")
    observed_delta = max(
        abs(r["observed"] - historical[r["id"]]["observed_radiance"]) for r in rows
    )
    if observed_delta > 1e-12:
        raise ValueError(f"Observed radiance disagreement {observed_delta}")
    checks = {
        "observation_ids": "exact",
        "cohort": len(rows),
        "evaluation": sum(r["split"] == "evaluation" for r in rows),
        "observed_max_abs_delta": observed_delta,
        "observed_tolerance": 1e-12,
        "byte_identical_historical_reproduction": False,
        "reason": "Current executable/build identity is distinct; comparison is numerical development evidence",
        "component_max_abs_delta": {},
        "residual_max_abs_delta": {},
    }
    for new, old in (("NSB Standard", "NSB"), ("nsb2", "nsb2")):
        values = {r["id"]: r for r in predictions["native"][new]}
        checks["component_max_abs_delta"][new] = {
            key: max(
                abs(
                    values[r["id"]]["components"][key]
                    - historical[r["id"]][old]["component_radiance"][key]
                )
                for r in rows
            )
            for key in ("zodiacal", "airglow", "starlight", "moonlight")
        }
        checks["residual_max_abs_delta"][new] = max(
            abs(
                (values[r["id"]]["total"] - r["observed"])
                - (
                    historical[r["id"]][old]["predicted_radiance"]
                    - historical[r["id"]]["observed_radiance"]
                )
            )
            for r in rows
        )
    if diagnostic and diagnostic_metrics:
        diag = load(diagnostic, "diagnostic")
        old_metrics = load(diagnostic_metrics, "diagnostic_metrics")
        by_id = {r["id"]: r for r in diag}
        evaluation = [r for r in diag if r["split"] == "evaluation"]
        rebuilt = scores(
            [r["observed"] for r in evaluation],
            {k: np.array([r["total"][k] for r in evaluation]) for k in evaluation[0]["total"]},
            [r["night"] for r in evaluation],
        )
        old_scores = old_metrics["scores_historical_evaluation_partition"]
        mapping = {"relative_bias": "mean_relative_error"}
        score_deltas = []
        for model, value in rebuilt["models"].items():
            for metric in (
                "mae",
                "rmse",
                "median_absolute_error",
                "mean_absolute_relative_error",
                "median_absolute_relative_error",
                "relative_bias",
            ):
                score_deltas.append(
                    abs(value[metric] - old_scores["models"][model][mapping.get(metric, metric)])
                )
        paired_deltas = []
        for pair, value in rebuilt["paired"].items():
            old = old_scores["paired"][pair]
            paired_deltas.extend(
                abs(np.array(value["relative_mae_ci95"]) - old["relative_mae_ci95"])
            )
            paired_deltas.extend(abs(np.array(value["mae_ci95"]) - old["mae_ci95"]))
        checks["historical_statistics_max_abs_delta"] = max(score_deltas)
        checks["historical_paired_ci_max_abs_delta"] = max(paired_deltas)
        if max(score_deltas + paired_deltas) > 1e-12:
            raise ValueError("Independent statistics disagree with frozen evidence")
        current_metrics = json.loads((run / "metrics.json").read_text())
        checks["current_metrics_max_abs_delta"] = {}
        checks["airglow_component_max_abs_delta"] = {}
        checks["new_vs_historical_totals"] = {}
        for scenario in predictions:
            suffix = "native" if scenario == "native" else "f27"
            for model, key in (
                ("NSB Standard", "continuum"),
                ("NSB Oxygen", "oxygen"),
                ("nsb2", "nsb2"),
            ):
                checks["new_vs_historical_totals"][f"{scenario}/{model}"] = max(
                    abs(r["total"] - by_id[r["id"]]["total"][f"{key}_{suffix}"])
                    for r in predictions[scenario][model]
                )
                label = f"{scenario}/{model}"
                if model != "nsb2":
                    checks["airglow_component_max_abs_delta"][label] = max(
                        abs(
                            r["components"]["airglow"]
                            - by_id[r["id"]]["airglow"][f"{key}_{suffix}"]
                        )
                        for r in predictions[scenario][model]
                    )
                new = current_metrics[scenario]["models"][model]
                old = old_scores["models"][f"{key}_{suffix}"]
                checks["current_metrics_max_abs_delta"][label] = max(
                    abs(new[k] - old[mapping.get(k, k)])
                    for k in (
                        "mean_signed_error",
                        "mae",
                        "rmse",
                        "median_absolute_error",
                        "mean_absolute_relative_error",
                        "median_absolute_relative_error",
                        "relative_bias",
                    )
                )
        checks["solar_max_abs_delta"] = max(
            abs(r["f27"] - by_id[r["id"]]["observed_f107_centered_27d"]) for r in rows
        )
    checks["nsb_commit"] = manifest["nsb"].get("commit")
    return checks


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--run", type=Path, required=True)
    p.add_argument("--benchmark", type=Path, required=True)
    p.add_argument("--diagnostic", type=Path)
    p.add_argument("--diagnostic-metrics", type=Path)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    if a.output.exists():
        raise FileExistsError(a.output)
    if (
        a.run.resolve() == a.output.resolve().parent
        or a.run.resolve() in a.output.resolve().parents
    ):
        raise ValueError("Reference comparison must not alter a sealed report")
    json_write(a.output, compare(a.run, a.benchmark, a.diagnostic, a.diagnostic_metrics))
    print(a.output.read_text())


if __name__ == "__main__":
    main()
