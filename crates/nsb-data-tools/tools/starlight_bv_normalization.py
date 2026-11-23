# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Validation-only coherent uncertainty for the pinned B/V photon normalizer."""

import math
import statistics
from collections import defaultdict

ROUTES = {
    "full": (True, True),
    "g_and_bp": (True, False),
    "g_and_rp": (False, True),
    "g_only": (False, False),
}
LIMIT_G = 17.65


def regional_envelope(rows, route):
    """Absolute fractional bias vs prediction plus its delete-region jackknife SE."""
    regions = defaultdict(list)
    for row in rows:
        if row[route + "_photon"]:
            pixel = int(row["galactic_nest16"])
            regions[pixel // 256].append(
                (pixel, float(row["xp_photon_flux"]), float(row[route + "_photon"]))
            )
    sample = [row for region in regions.values() for row in region]
    sky_cells = len({r[0] for r in sample})
    if len(sample) < 32 or sky_cells < 5 or len(regions) < 2:
        return None
    measured = math.fsum(r[1] for r in sample)
    predicted = math.fsum(r[2] for r in sample)

    def discrepancy(records):
        p = math.fsum(r[2] for r in records)
        return math.fsum(r[2] - r[1] for r in records) / p

    bias = discrepancy(sample)
    leave_one_out = [
        discrepancy(
            [row for key, region in regions.items() if key != sky for row in region]
        )
        for sky in regions
    ]
    center = statistics.mean(leave_one_out)
    n = len(leave_one_out)
    jackknife_se = math.sqrt(
        (n - 1) / n * math.fsum((v - center) ** 2 for v in leave_one_out)
    )
    return {
        "sources": len(sample),
        "nside16_cells": sky_cells,
        "nside1_blocks": n,
        "predicted_photon_sum": predicted,
        "measured_photon_sum": measured,
        "relative_bias_vs_prediction": bias,
        "block_jackknife_se": jackknife_se,
        "fraction": abs(bias) + jackknife_se,
    }


def fit_envelopes(rows, edges):
    # Outside the main XP limit the known sample has poor support and strong
    # selection effects. Use the largest supported envelope at application.
    validation = [r for r in rows if float(r["g"]) < LIMIT_G]
    audit = {}
    table = [{} for _ in edges[:-1]]
    for route in ROUTES:
        pooled = regional_envelope(validation, route)
        if pooled is None:
            raise ValueError(
                f"insufficient validation sky support for the {route} normalization envelope"
            )
        bins = [
            regional_envelope(
                [r for r in validation if low <= float(r["g"]) < high], route
            )
            for low, high in zip(edges, edges[1:])
        ]
        for cell, estimate in zip(table, bins):
            cell[route] = (estimate or pooled)["fraction"]
        # Include the all-magnitude control in the extrapolation envelope.
        audit[route] = {
            "pooled": pooled,
            "magnitude_cells": bins,
            "maximum_supported_fraction": max(
                pooled["fraction"], *(cell[route] for cell in table)
            ),
        }
    return {
        "magnitude_cells": table,
        "extrapolation": {
            route: entry["maximum_supported_fraction"] for route, entry in audit.items()
        },
    }, audit


def normalization_metrics(entries):
    """Metrics on photon flux and the additive B/V bias decomposition."""
    if not entries:
        return {"evaluated": 0}
    # Each entry carries measured/predicted photons and the two monochromatic predictions.
    x, p, truth, shape_only, joint = zip(*entries)
    q = [a / b for a, b in zip(p, x)]
    q_sorted = sorted(q)
    centered = statistics.mean(q)
    # Conventional smearing diagnostic of the residual distribution. It is not
    # an estimator or an applied map correction.
    log_residuals = [math.log(a / b) for a, b in zip(x, p)]
    geometric_residual = math.exp(statistics.mean(log_residuals))
    weighted_ratio = math.fsum(p) / math.fsum(x)
    decomposition = []
    for i in range(2):
        t = [r[i] for r in truth]
        s = [r[i] for r in shape_only]
        j = [r[i] for r in joint]
        e1 = [a - b for a, b in zip(s, t)]
        e2 = [a - b for a, b in zip(j, s)]
        norm = math.fsum(t)
        correlation = (
            statistics.correlation(e1, e2)
            if len(e1) > 1 and min(e1) != max(e1) and min(e2) != max(e2)
            else None
        )
        decomposition.append(
            {
                "shape_bias_on_truth_sum": math.fsum(e1) / norm if norm > 0 else None,
                "normalization_bias_on_truth_sum": math.fsum(e2) / norm
                if norm > 0
                else None,
                "normalization_ratio_reweighted_by_predicted_shape": (
                    math.fsum(j) / math.fsum(s) if math.fsum(s) > 0 else None
                ),
                "pearson_of_signed_error_terms": correlation,
            }
        )
    return {
        "evaluated": len(entries),
        "measured_photon_sum": math.fsum(x),
        "predicted_photon_sum": math.fsum(p),
        "flux_weighted_bias": weighted_ratio - 1,
        "source_weighted_mean_ratio": centered,
        "source_weighted_ratio_quantiles": {
            str(k): q_sorted[math.ceil(k * len(q) / 100) - 1] for k in (10, 50, 90, 95)
        },
        "geometric_measured_to_predicted": geometric_residual,
        "source_weighted_smearing_about_geometric_ratio": (
            statistics.mean(1 / v for v in q) / geometric_residual
        ),
        "bv_bias_decomposition": decomposition,
    }
