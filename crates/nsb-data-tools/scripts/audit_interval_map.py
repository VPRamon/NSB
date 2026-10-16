# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Independent integer-accumulator, HEALPix and packed-uncertainty audit.

Run against a private copy of the completed map, reconciled shards and official
inventories. Original Gaia objects remain on Hydra; their per-worker verified
SHA-256 identities are checked against receipts, not downloaded again.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from collections import Counter
from fractions import Fraction
from pathlib import Path

import healpy as hp
import numpy as np


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def integer(accumulator):
    return sum(
        int(value) << (64 * int(key)) for key, value in accumulator["limbs"].items()
    )


def real(value):
    return float(Fraction(value, 1 << 1074))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--map-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples-output", type=Path, required=True)
    args = parser.parse_args()
    root = args.workspace
    path = root / "outputs/starlight_360_570_candidate.json"
    if digest(path) != args.map_sha256:
        raise ValueError("Frozen interval map checksum mismatch")
    product = json.loads(path.read_bytes())
    expected = set(product["input_partitions"])
    for name in ["gaia-source", "xp-continuous"]:
        inventory = json.loads(
            (root / "inventories" / f"{name}.inventory.json").read_bytes()
        )
        if {r["partition_id"] for r in inventory["entries"]} != expected:
            raise ValueError("Map does not cover the complete official inventory")
    if len(expected) != 3386:
        raise ValueError("Gaia DR3 full inventory must contain 3386 paired partitions")
    pixels = {}
    excluded = Counter()
    catalogue, xp_rows, unmatched = 0, 0, 0
    samples = []
    # Reverse partition traversal is deliberately independent of canonical merge order.
    for partition in sorted(expected, reverse=True):
        path = root / "outputs/shards" / f"{partition}.json"
        sha = digest(path)
        if sha != product["input_partitions"][partition]["shard_sha256"]:
            raise ValueError("Shard identity mismatch")
        shard = json.loads(path.read_bytes())
        if shard["design_sha256"] != product["design_sha256"]:
            raise ValueError("Mixed XP calibration designs")
        if (
            shard["band_nm"] != [360, 570]
            or shard["nside"] != 128
            or shard["partition"] != partition
        ):
            raise ValueError("Shard spectral/geometrical identity mismatch")
        for name, sha in shard["input_sha256"].items():
            receipt = json.loads(
                (root / "cache/receipts" / name / f"{partition}.json").read_bytes()
            )
            if (
                sha != receipt["sha256"]
                or sha != product["input_partitions"][partition]["inputs"][name]
            ):
                raise ValueError(
                    "Worker source identity differs from acquisition receipt"
                )
        catalogue += shard["catalogue_rows"]
        xp_rows += shard["xp_rows"]
        unmatched += shard["unmatched_xp_rows"]
        excluded.update(shard["excluded"])
        samples.extend(shard["training_samples"])
        for index, data in shard["pixels"].items():
            entry = pixels.setdefault(
                int(index),
                {
                    "flux": [0] * 7,
                    "variance": [0] * 7,
                    "full_variance": 0,
                    "sources": 0,
                },
            )
            for i in range(7):
                entry["flux"][i] += integer(data["flux_ph_m2_s"][i])
                entry["variance"][i] += integer(data["variance_ph_m2_s_squared"][i])
            entry["full_variance"] += integer(
                data["integrated_variance_ph_m2_s_squared"]
            )
            entry["sources"] += data["sources"]
    # Official Gaia DR3 gaia_source total (Gaia Collaboration DR3 summary).
    if catalogue != 1_811_709_771:
        raise ValueError("GaiaSource row total differs from the published DR3 population")
    factor = hp.nside2npix(128) / (4 * math.pi) * 1e-13
    maximum = 0.0
    admitted = 0
    source_flux = 0
    for index, packed in enumerate(product["pixels"]):
        nested = int(hp.ring2nest(128, index))
        value = pixels.get(
            nested,
            {"flux": [0] * 7, "variance": [0] * 7, "full_variance": 0, "sources": 0},
        )
        if packed["pixel"] != index or packed["sources"] != value["sources"]:
            raise ValueError("Packed RING source-count mismatch")
        reference_flux = np.array([real(x) * factor for x in value["flux"]])
        reference_sigma = np.array(
            [math.sqrt(real(x)) * factor for x in value["variance"]]
        )
        for actual, reference in [
            (np.array(packed["radiance"]), reference_flux),
            (np.array(packed["statistical_sigma"]), reference_sigma),
            (
                np.array([packed["integrated_statistical_sigma"]]),
                np.array([math.sqrt(real(value["full_variance"])) * factor]),
            ),
        ]:
            if not np.all(np.isfinite(actual)) or np.any(actual < 0):
                raise ValueError("Nonphysical packed channel")
            delta = np.max(
                np.abs(actual - reference) / np.maximum(np.abs(reference), 1e-300)
            )
            maximum = max(maximum, float(delta))
            if delta > 5e-14:
                raise ValueError(
                    "Packed radiance/uncertainty differs from exact independent merge"
                )
        admitted += value["sources"]
        source_flux += sum(value["flux"])
    if (
        admitted != product["admitted_sources"]
        or admitted + sum(excluded.values()) != catalogue
    ):
        raise ValueError("Full population accounting failed")
    if (
        catalogue != product["catalogue_rows"]
        or xp_rows != product["xp_rows"]
        or unmatched != product["unmatched_xp_rows"]
        or dict(excluded) != product["excluded_sources"]
    ):
        raise ValueError("Reported population identities differ")
    flux = np.array([sum(p["radiance"]) for p in product["pixels"]])
    _longitude, latitude = hp.pix2ang(128, np.arange(len(flux)), lonlat=True)
    source_total = real(source_flux)
    packed_total = float(np.sum(flux)) / factor
    error = abs(packed_total / source_total - 1)
    if error > 5e-14:
        raise ValueError("Global flux conservation failed")
    report = {
        "schema": "nsb-starlight-independent-map-audit-v1",
        "map_sha256": args.map_sha256,
        "passed": True,
        "expected_partitions": 3386,
        "processed_partitions": len(expected),
        "independent_reverse_merge": True,
        "healpy_version": hp.__version__,
        "source_flux_ph_m2_s": source_total,
        "packed_flux_ph_m2_s": packed_total,
        "relative_flux_conservation_error": error,
        "maximum_packed_relative_difference": maximum,
        "admitted_sources": admitted,
        "catalogue_rows": catalogue,
        "xp_rows": xp_rows,
        "excluded_sources": dict(excluded),
        "unmatched_xp_rows": unmatched,
        "covered_pixels": int(np.sum(flux > 0)),
        "total_pixels": len(flux),
        "galactic_plane_median_radiance": float(np.median(flux[abs(latitude) < 10])),
        "galactic_poles_median_radiance": float(np.median(flux[abs(latitude) > 60])),
        "radiance_p0_p50_p90_p99_p100": np.percentile(
            flux, [0, 50, 90, 99, 100]
        ).tolist(),
        "training_sample_sources": len(samples),
        "uncertainty_limitation": "source correlations outside each BP spectrum and calibration systematics unquantified",
        "source_integrity": "all source hashes tied to Rust worker SHA-256 verification and acquisition receipts; original catalogue objects not re-downloaded",
    }
    with args.output.open("x") as f:
        json.dump(report, f, indent=2, allow_nan=False)
        f.write("\n")
    samples.sort(key=lambda r: int(r["source_id"]))
    with args.samples_output.open("x") as f:
        json.dump(
            {
                "schema": "nsb-xp-interval-calibration-sample-v1",
                "expected_partitions": 3386,
                "processed_partitions": len(expected),
                "map_sha256": args.map_sha256,
                "sampling_rule": "admitted source_id modulo4093 equals zero",
                "samples": samples,
            },
            f,
            allow_nan=False,
        )


if __name__ == "__main__":
    main()
