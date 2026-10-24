# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Equal-spectrum estimands, paired night-cluster percentile bootstrap."""

from itertools import combinations

import numpy as np


def align(rows, predictions):
    """Exact identifier matching; reject duplicates, extras, missing/nonfinite predictions."""
    ids = [r["id"] for r in rows]
    if not ids or len(set(ids)) != len(ids):
        raise ValueError("Unique nonempty observation identities required")
    result = {}
    missing = {}
    for model, records in predictions.items():
        mapping = {r["id"]: r for r in records}
        missing[model] = sorted(set(ids) - set(mapping))
        if len(mapping) != len(records) or set(mapping) != set(ids):
            raise ValueError(f"{model}: cohort mismatch; missing={missing[model]}")
        values = np.array([mapping[key]["total"] for key in ids], float)
        if not np.isfinite(values).all() or np.any(values < 0):
            raise ValueError(f"{model}: invalid predictions")
        result[model] = values
    return result


def baseline(rows):
    train = [r for r in rows if r["split"] == "calibration"]
    test = [r for r in rows if r["split"] == "evaluation"]
    if not train or not test or set(r["night"] for r in train) & set(r["night"] for r in test):
        raise ValueError("Disjoint nonempty calibration and evaluation nights required")
    return float(np.median([r["observed"] for r in train]))


def errors(observed, predicted):
    y, p = np.asarray(observed, float), np.asarray(predicted, float)
    if (
        y.ndim != 1
        or y.size == 0
        or y.shape != p.shape
        or not np.isfinite(y).all()
        or not np.isfinite(p).all()
        or np.any(y <= 0)
    ):
        raise ValueError("Finite aligned vectors and positive observations required")
    d = p - y
    rel = d / y
    with np.errstate(over="raise", invalid="raise", divide="raise"):
        values = {
            "mean_signed_error": np.mean(d),
            "mae": np.mean(abs(d)),
            "rmse": np.linalg.norm(d / np.sqrt(len(d))),
            "median_absolute_error": np.median(abs(d)),
            "mean_absolute_relative_error": np.mean(abs(rel)),
            "median_absolute_relative_error": np.median(abs(rel)),
            "relative_bias": np.mean(rel),
        }
    if not np.isfinite(list(values.values())).all():
        raise ValueError("Unstable metric values")
    return {k: float(v) for k, v in values.items()}


def scores(observed, predictions, nights, repetitions=2000, seed=182):
    y, nights = np.asarray(observed, float), np.asarray(nights)
    if nights.shape != y.shape or repetitions < 100 or not predictions:
        raise ValueError("Matching nights, models and >=100 repetitions required")
    groups = [np.flatnonzero(nights == n) for n in np.unique(nights)]
    if len(groups) < 3:
        raise ValueError("At least three independent nights required for intervals")
    predictions = {k: np.asarray(v, float) for k, v in predictions.items()}
    metrics = {
        k: {**errors(y, p), "n": len(y), "nights": len(groups), "missing": 0}
        for k, p in predictions.items()
    }
    rng = np.random.default_rng(seed)
    draws = {k: np.empty((repetitions, 2)) for k in predictions}
    for i in range(repetitions):
        indices = np.concatenate([groups[j] for j in rng.integers(len(groups), size=len(groups))])
        for key, p in predictions.items():
            delta = abs(p[indices] - y[indices])
            draws[key][i] = (np.mean(delta), np.mean(delta / y[indices]))
    for key in predictions:
        metrics[key]["mae_ci95"] = np.percentile(draws[key][:, 0], [2.5, 97.5]).tolist()
        metrics[key]["relative_mae_ci95"] = np.percentile(draws[key][:, 1], [2.5, 97.5]).tolist()
    paired = {}
    for a, b in combinations(predictions, 2):
        paired[f"{a}_minus_{b}"] = {
            "mae_difference": metrics[a]["mae"] - metrics[b]["mae"],
            "relative_mae_difference": metrics[a]["mean_absolute_relative_error"]
            - metrics[b]["mean_absolute_relative_error"],
            "mae_ci95": np.percentile(draws[a][:, 0] - draws[b][:, 0], [2.5, 97.5]).tolist(),
            "relative_mae_ci95": np.percentile(
                draws[a][:, 1] - draws[b][:, 1], [2.5, 97.5]
            ).tolist(),
        }
    return {
        "models": metrics,
        "paired": paired,
        "repetitions": repetitions,
        "seed": seed,
        "estimand": "equal weight per spectrum (historically called exposure-weighted); resample whole nights",
        "interval_scope": "sampling variability conditional on fixed calibration baseline; no calibration-systematic uncertainty",
    }
