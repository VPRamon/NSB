# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Spatially held-out feasibility fit for non-XP interval inference.

This produces a research artifact, not a production-admitted calibration. It
uses only Gaia XP photon targets and catalogue photometry, never DESI residuals.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--samples", type=Path, required=True)
    p.add_argument("--samples-sha256", required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    content = a.samples.read_bytes()
    if hashlib.sha256(content).hexdigest() != a.samples_sha256:
        raise ValueError("Frozen measured-XP sample identity required")
    document = json.loads(content)
    rows = document["samples"]
    if document["processed_partitions"] != document["expected_partitions"]:
        raise ValueError("Complete partition sample collection required")
    selected = []
    for row in rows:
        if any(row[k] is None for k in ["g", "bp", "rp", "bp_rp"]):
            continue
        flux = np.array(row["flux_ph_m2_s"])
        variance = np.array(row["variance"])
        if not 6 <= row["g"] <= 19 or not -0.3 <= row["bp_rp"] <= 4.0:
            continue
        if (
            np.any(flux <= 0)
            or np.any(variance < 0)
            or flux[0] <= 50 * np.sqrt(variance[0])
        ):
            continue
        selected.append(row)
    if len(selected) < 1000:
        raise ValueError("Insufficient high-S/N calibration stars")
    ids = [r["source_id"] for r in selected]
    if len(set(ids)) != len(ids):
        raise ValueError("Duplicate calibration source")
    # NSIDE=64 parent regions of Galactic NESTED NSIDE=128 accumulation.
    region = [int(r["pixel"]) // 4 for r in selected]
    role = np.array(
        [
            int(hashlib.sha256(f"184:{r}".encode()).hexdigest()[:8], 16) % 10
            for r in region
        ]
    )
    train, validation, test = role < 6, (role >= 6) & (role < 8), role >= 8
    g = np.array([r["g"] for r in selected])
    flux = np.array([r["flux_ph_m2_s"] for r in selected])
    # Remove the exact Pogson brightness scale before fitting spectral colour.
    target = np.log(flux) + (0.4 * np.log(10)) * g[:, None]
    fits = {}
    for name, colour in {
        "g_bp_rp": np.array([r["bp_rp"] for r in selected]),
        "g_bp": np.array([r["bp"] - r["g"] for r in selected]),
        "g_rp": np.array([r["g"] - r["rp"] for r in selected]),
    }.items():
        design = np.column_stack([colour**i for i in range(4)])
        beta, _, rank, _ = np.linalg.lstsq(design[train], target[train], rcond=None)
        if rank != 4:
            raise ValueError("Singular colour calibration design")
        predicted = np.exp(design @ beta - (0.4 * np.log(10)) * g[:, None])
        by_role = {}
        for label, mask in [
            ("training", train),
            ("validation", validation),
            ("test", test),
        ]:
            relative = predicted[mask] / flux[mask] - 1
            by_role[label] = {
                "sources": int(np.sum(mask)),
                "regions": len(set(np.array(region)[mask].tolist())),
                "bias_relative": np.mean(relative, axis=0).tolist(),
                "mae_relative": np.mean(abs(relative), axis=0).tolist(),
                "median_absolute_relative": np.median(abs(relative), axis=0).tolist(),
                "p95_absolute_relative": np.percentile(
                    abs(relative), 95, axis=0
                ).tolist(),
            }
        residual = target[train] - design[train] @ beta
        scatter = np.std(residual, axis=0, ddof=4)
        covariance = np.linalg.inv(design[train].T @ design[train])
        fits[name] = {
            "parameters": beta.T.tolist(),
            "parameter_covariance": [
                (covariance * sigma**2).tolist() for sigma in scatter
            ],
            "log_residual_sigma": scatter.tolist(),
            "colour_training_domain": [
                float(np.min(colour[train])),
                float(np.max(colour[train])),
            ],
            "heldout_validation": by_role,
        }
    artifact = {
        "schema": "nsb-starlight-interval-photometric-research-v1",
        "status": "research_only_not_runtime_admitted",
        "band_nm": [360, 570],
        "target_intervals_nm": [[360, 570]]
        + [[360 + 30 * i, 390 + 30 * i] for i in range(7)],
        "response": "median_flux=exp(cubic_colour-0.4*ln(10)*G)",
        "source_samples_sha256": a.samples_sha256,
        "sample_policy": "source_id modulo 4093 equals zero; admitted positive-subbands XP only",
        "quality_policy": "6<=G<=19; -0.3<=BP-RP<=4; integrated XP S/N>50",
        "role_policy": "Galactic NSIDE64 parent regions; SHA256(184:region) modulo10; 60/20/20",
        "selected_sources": len(selected),
        "source_ids_sha256": hashlib.sha256(
            "\n".join(sorted(ids)).encode()
        ).hexdigest(),
        "numpy_version": np.__version__,
        "fits": fits,
        "missing_uncertainty": "source-correlated catalogue/calibration systematics remain unquantified",
        "limitations": [
            "Does not validate transport or DESI angular response",
            "XP selection and high-S/N training can differ from faint non-XP populations",
            "G-only colour prior is not admitted",
            "No extrapolation outside learned colour domains is justified",
            "OLS covariance assumes homoskedastic log residuals",
            "Positive sub-band selection can bias training targets",
        ],
    }
    with a.output.open("x") as f:
        json.dump(artifact, f, indent=2, allow_nan=False)
        f.write("\n")


if __name__ == "__main__":
    main()
