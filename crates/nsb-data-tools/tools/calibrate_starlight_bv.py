#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Fit and audit candidate B/V conditional moments from nsb-data diagnose bv-xp."""

import argparse
from bisect import bisect_right
from collections import defaultdict
import csv
import hashlib
import json
import math
from pathlib import Path
import shlex
import statistics
import sys
import tempfile


REFERENCES = ("b_w_m2_nm", "v_w_m2_nm")
SPLITS = ("training", "validation", "test")
ROUTES = {(True, True): "full", (True, False): "g_and_bp",
          (False, True): "g_and_rp", (False, False): "g_only"}
SUPPORT = ("min_training", "min_validation", "min_validation_sky_cells")
EDGES = ("g_edges", "bp_g_edges", "g_rp_edges")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()


def write_json(path, data):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        stream.write(data)
    try:
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def load_policy(path, exporting):
    raw = path.read_bytes()
    policy = json.loads(raw)
    required = {"sky_nside", "seed", *EDGES, *SUPPORT}
    provenance = {"software_commit", "reference_provenance", "xp_design_sha256",
                  "xp_shared_calibration_fraction"}
    if not isinstance(policy, dict) or not required <= policy.keys() or policy.keys() - required - provenance:
        raise ValueError("policy has missing or unexpected keys")
    if type(policy["seed"]) is not int or not 0 <= policy["seed"] < 2**64:
        raise ValueError("seed must be an unsigned 64-bit integer")
    if type(policy["sky_nside"]) is not int or policy["sky_nside"] not in (1, 2, 4, 8):
        raise ValueError("policy needs sky_nside 1/2/4/8")
    if any(type(policy[key]) is not int or not 2 <= policy[key] < 2**64 for key in SUPPORT):
        raise ValueError("policy needs at least two sources and sky cells for variance")
    for key in EDGES:
        edges = policy[key]
        if (not isinstance(edges, list) or len(edges) < 2
                or any(type(x) not in (int, float) or not math.isfinite(x) for x in edges)
                or edges != sorted(set(edges))):
            raise ValueError(f"invalid {key}")
    if exporting:
        if not provenance <= policy.keys():
            raise ValueError("candidate export requires label provenance, software and XP design pins")
        for key, size in (("software_commit", 40), ("xp_design_sha256", 64)):
            value = policy[key]
            if not isinstance(value, str) or len(value) != size or any(x not in "0123456789abcdef" for x in value):
                raise ValueError(f"invalid {key}")
        description = policy["reference_provenance"]
        fractions = policy["xp_shared_calibration_fraction"]
        if not isinstance(description, str) or not description.strip():
            raise ValueError("reference_provenance is required")
        if (not isinstance(fractions, list) or len(fractions) != 2
                or any(type(x) not in (int, float) or not math.isfinite(x) or x <= 0 for x in fractions)):
            raise ValueError("invalid XP shared calibration fractions")
    return policy, sha(raw)


def split_for(seed, fine_pixel):
    bucket = int(sha(f"{seed}:{fine_pixel}".encode())[:16], 16) % 10
    return SPLITS[0] if bucket < 6 else SPLITS[1] if bucket < 8 else SPLITS[2]


def bin_for(edges, value):
    index = bisect_right(edges, value) - 1
    return index if 0 <= index < len(edges) - 1 else None


def cells_for(row, policy, stress_test):
    g, bp, rp = (float(row[x]) if row[x] else None for x in ("g", "bp", "rp"))
    if any(value is not None and not math.isfinite(value) for value in (g, bp, rp)):
        raise ValueError(f"non-finite Gaia photometry for source {row['source_id']}")
    targets = [float(row[name]) for name in REFERENCES]
    if not all(map(math.isfinite, targets)):
        raise ValueError(f"non-finite B/V reference for source {row['source_id']}")
    if g is None:
        return
    masks = [(bp is not None, rp is not None)]
    if stress_test and all(masks[0]):
        masks.extend([(True, False), (False, True), (False, False)])
    g_bin = bin_for(policy["g_edges"], g)
    if g_bin is None:
        return
    sky_pixel = int(row["galactic_nest16"]) // (16 // policy["sky_nside"]) ** 2
    scale = 10 ** (-0.4 * (g - 15))
    if scale == 0.0:
        raise ValueError(f"G normalization underflow for source {row['source_id']}")
    normalized = [target / scale for target in targets]
    if not all(map(math.isfinite, normalized)):
        raise ValueError(f"non-finite B/V reference for source {row['source_id']}")
    for has_bp, has_rp in masks:
        bp_bin = bin_for(policy["bp_g_edges"], bp - g) if has_bp else None
        rp_bin = bin_for(policy["g_rp_edges"], g - rp) if has_rp else None
        if (has_bp and bp_bin is None) or (has_rp and rp_bin is None):
            continue
        yield (ROUTES[has_bp, has_rp], sky_pixel, g_bin, bp_bin, rp_bin), normalized, scale


def fit(groups, policy):
    cells = {}
    for key, splits in groups.items():
        train, validation = splits["training"], splits["validation"]
        if len(train) < policy["min_training"] or len(validation) < policy["min_validation"]:
            continue
        columns = list(zip(*train))
        mean = [statistics.mean(x) for x in columns]
        var = [statistics.variance(x) for x in columns]
        if min(mean) <= 0 or min(var) <= 0:
            continue
        cross = statistics.covariance(*columns)
        # Bound the correlated cell bias using the held-out mean residual and
        # its 95% standard-error interval. Repeated sky cells can correlate,
        # so treat each validation cell as one unit when estimating the error.
        by_pixel = defaultdict(list)
        for pixel, target, _ in validation:
            by_pixel[pixel].append(target)
        if len(by_pixel) < policy["min_validation_sky_cells"]:
            continue
        means = [tuple(map(statistics.mean, zip(*values))) for values in by_pixel.values()]
        absolute_fraction = []
        for i in range(2):
            residual = [x[i] / mean[i] - 1 for x in means]
            absolute_fraction.append(abs(statistics.mean(residual))
                                     + 1.96 * statistics.stdev(residual) / math.sqrt(len(residual)))
        if min(absolute_fraction) <= 0:
            continue
        cells[key] = {
            "key": dict(zip(("route", "sky_pixel", "g_bin", "bp_g_bin", "g_rp_bin"), key)),
            "mean_w_m2_nm_at_g15": mean,
            "residual_covariance_at_g15": [*var, cross],
            "shared_fraction": absolute_fraction,
            "training_sources": len(train),
            "validation_sources": len(validation),
            "validation_sky_cells": len(by_pixel),
        }
    return cells


def metrics(test):
    if not test:
        return {"evaluated": 0}
    bands = []
    for values in zip(*test):
        truth, prediction, error = zip(*values)
        relative = [(p - t) / t for t, p in zip(truth, prediction) if t > 0]
        absolute = sorted(map(abs, relative))
        total = math.fsum(truth)
        bands.append({
            "relative_bias_sum": math.fsum(prediction) / total - 1 if total > 0 else None,
            "mae_w_m2_nm": statistics.mean(abs(p-t) for t, p in zip(truth, prediction)),
            "rmse_w_m2_nm": math.sqrt(statistics.mean((p-t)**2 for t, p in zip(truth, prediction))),
            "median_signed_relative_error": statistics.median(relative) if relative else None,
            "absolute_relative_error_percentiles": {
                str(p): absolute[math.ceil(len(absolute) * p / 100) - 1] if absolute else None
                for p in (50, 68, 90, 95, 99)
            },
            "relative_error_positive_references": len(absolute),
            "coverage_68": sum(abs(p-t) <= e for t, p, e in values) / len(values),
            "coverage_95": sum(abs(p-t) <= 1.96*e for t, p, e in values) / len(values),
        })
    return {"evaluated": len(test), "bands_b_v": bands}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("references", type=Path)
    parser.add_argument("policy", type=Path)
    parser.add_argument("report", type=Path)
    parser.add_argument("--candidate-calibration", type=Path)
    parser.add_argument("--stress-test-mask-complete", action="store_true")
    args = parser.parse_args()
    if args.stress_test_mask_complete and args.candidate_calibration:
        parser.error("masked photometry is for stress tests and cannot be exported as a calibration")
    if len({p.resolve() for p in (args.references, args.policy, args.report, args.candidate_calibration) if p}) != (4 if args.candidate_calibration else 3):
        parser.error("inputs and output paths must be distinct")
    try:
        policy, policy_sha = load_policy(args.policy, bool(args.candidate_calibration))
    except ValueError as error:
        parser.error(str(error))

    groups = defaultdict(lambda: defaultdict(list))
    sky_splits = defaultdict(set)
    id_splits = defaultdict(set)
    seen = set()
    test_sources_without_bins = 0
    # Hash exactly the bytes that are parsed.
    reference_bytes = args.references.read_bytes()
    reader = csv.DictReader(reference_bytes.decode().splitlines())
    columns = {"source_id", "galactic_nest16", "g", "bp", "rp", *REFERENCES}
    if not reader.fieldnames or not columns <= set(reader.fieldnames) or len(reader.fieldnames) != len(set(reader.fieldnames)):
        parser.error("reference CSV requires unique source, nside-16 sky, photometry and flux columns")
    for row in reader:
        try:
            if None in row or any(row[key] is None for key in columns):
                raise ValueError("incomplete or extra CSV fields")
            source_id, fine_pixel = int(row["source_id"]), int(row["galactic_nest16"])
            if source_id in seen or not 0 < source_id < 2**64 or not 0 <= fine_pixel < 12 * 16**2:
                raise ValueError(f"duplicate/invalid source or invalid sky cell for {source_id}")
            entries = list(cells_for(row, policy, args.stress_test_mask_complete))
        except (ValueError, TypeError, OverflowError) as error:
            parser.error(f"reference CSV line {reader.line_num}: {error}")
        seen.add(source_id)
        split = split_for(policy["seed"], fine_pixel)
        sky_splits[split].add(fine_pixel)
        id_splits[split].add(source_id)
        if split == "test" and not entries:
            test_sources_without_bins += 1
        for key, values, scale in entries:
            groups[key][split].append(values if split == "training" else (fine_pixel, values, scale))
    if not seen:
        parser.error("reference CSV is empty")

    cells = fit(groups, policy)
    scored = defaultdict(list)
    attempted = defaultdict(int)
    for key, values in groups.items():
        route, sky = key[:2]
        attempted[route, sky] += len(values["test"])
        if key not in cells:
            continue
        model = cells[key]
        mean, variance = model["mean_w_m2_nm_at_g15"], model["residual_covariance_at_g15"]
        sigmas = [math.hypot(math.sqrt(variance[i]), mean[i] * model["shared_fraction"][i])
                  for i in range(2)]
        scored[route, sky].extend(
            tuple((t * scale, p * scale, e * scale) for t, p, e in zip(truth, mean, sigmas))
            for _, truth, scale in values["test"]
        )

    manifest = {
        split: {"partition_id": split, "source_count": len(id_splits[split]),
                "source_ids_sha256": sha(json_bytes(sorted(id_splits[split]))),
                "sky_regions": [str(x) for x in sorted(sky_splits[split])]}
        for split in SPLITS
    }
    references_sha = sha(reference_bytes)
    report = {
        "policy": policy, "policy_sha256": policy_sha, "references_sha256": references_sha,
        "test_sources_without_bins": test_sources_without_bins,
        "masked_stress_test": args.stress_test_mask_complete,
        "splits": manifest, "fitted_cell_count": len(cells), "candidate_cell_count": len(groups),
        "regions": [{"route": route, "galactic_nested_pixel": sky, "test_sources": attempted[route, sky],
                     **metrics(scored[route, sky])} for route, sky in sorted(attempted)],
    }
    report_bytes = json_bytes(report)
    write_json(args.report, report_bytes)
    if args.candidate_calibration:
        fitted_routes = {key[0] for key in cells}
        tested_routes = {route for (route, _), entries in scored.items() if entries}
        if len(fitted_routes) != 4 or tested_routes != fitted_routes:
            parser.error("insufficient training, validation or held-out test support across the four routes")
        artifact = {
            "schema_version": 1, "calibration_status": "candidate", "reference_nm": [445.0, 551.0],
            "reference_provenance": policy["reference_provenance"], "policy_sha256": policy_sha,
            "held_out_report_sha256": sha(report_bytes),
            "xp_design_sha256": policy["xp_design_sha256"],
            "xp_shared_calibration_fraction": policy["xp_shared_calibration_fraction"],
            "software_commit": policy["software_commit"], "training_command": shlex.join(sys.argv),
            **{name: policy[name] for name in ("sky_nside", *EDGES, *SUPPORT)},
            "cells": list(cells.values()),
            "partitions": {**manifest, "assignment_algorithm": "sha256-galactic-nested-nside16-mod10-v1",
                           "seed": policy["seed"], "manifest_sha256": references_sha,
                           "source_disjoint": True, "sky_disjoint": True,
                           "disjointness_evidence_sha256": sha(json_bytes(manifest))},
        }
        write_json(args.candidate_calibration, json_bytes(artifact))


if __name__ == "__main__":
    main()
