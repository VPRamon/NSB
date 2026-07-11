#!/usr/bin/env python3
"""Offline GaiaXPy calibration for XP continuous coefficient CSV files."""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path

import gaiaxpy
import numpy as np
import pandas as pd

BAND_MIN_NM = 336.0
BAND_MAX_NM = 650.0
GRID_STEP_NM = 2.0
GAIA_XPY_VERSION = gaiaxpy.__version__
PHOTOMETRY_MODEL = "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sampling_grid() -> np.ndarray:
    # Inclusive upper bound matches Rust XP sampled integration (336–650 nm, 2 nm step).
    return np.arange(BAND_MIN_NM, BAND_MAX_NM + GRID_STEP_NM * 0.5, GRID_STEP_NM)


def format_series(values: np.ndarray, scientific: bool) -> str:
    parts = []
    for value in values:
        if not np.isfinite(value):
            raise ValueError("non-finite calibrated flux sample")
        if scientific:
            parts.append(f"{float(value):.8e}")
        else:
            parts.append(f"{float(value):.8f}")
    return ";".join(parts)


def write_normalized_csv(
    output_path: Path,
    source_id: str,
    wavelengths_nm: np.ndarray,
    flux_w_m2_nm: np.ndarray,
    flux_error_w_m2_nm: np.ndarray,
) -> None:
    part = output_path.with_suffix(output_path.suffix + ".part")
    header = "source_id,xp_wavelength_nm,xp_flux_w_m2_nm,xp_flux_error_w_m2_nm\n"
    row = (
        f"{source_id},"
        f"{format_series(wavelengths_nm, False)},"
        f"{format_series(flux_w_m2_nm, True)},"
        f"{format_series(flux_error_w_m2_nm, True)}\n"
    )
    part.write_text(header + row, encoding="utf-8")
    part.replace(output_path)


def reconstruct_one(coefficient_path: Path, output_path: Path) -> dict:
    if output_path.exists():
        return {
            "source_id": coefficient_path.stem,
            "status": "skipped_existing",
            "output_sha256": sha256_file(output_path),
        }

    sampling = sampling_grid()
    calibrated, _correlation = gaiaxpy.calibrate(
        str(coefficient_path),
        sampling=sampling,
        save_file=False,
        truncation=False,
    )
    if len(calibrated) != 1:
        raise RuntimeError(f"expected one calibrated row for {coefficient_path}")
    row = calibrated.iloc[0]
    source_id = str(row["source_id"])
    flux = np.asarray(row["flux"], dtype=float)
    flux_error = np.asarray(row["flux_error"], dtype=float)
    if flux.shape != sampling.shape or flux_error.shape != sampling.shape:
        raise RuntimeError(f"calibrated grid mismatch for {source_id}")

    write_normalized_csv(output_path, source_id, sampling, flux, flux_error)
    return {
        "source_id": source_id,
        "status": "reconstructed",
        "coefficient_sha256": sha256_file(coefficient_path),
        "output_sha256": sha256_file(output_path),
        "band_nm": [BAND_MIN_NM, BAND_MAX_NM],
        "grid_step_nm": GRID_STEP_NM,
        "samples": int(len(sampling)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coefficients-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=None)
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    coefficient_paths = sorted(args.coefficients_dir.glob("*.csv"))
    if args.limit is not None:
        coefficient_paths = coefficient_paths[: args.limit]

    entries = []
    for coefficient_path in coefficient_paths:
        output_path = args.output_dir / f"{coefficient_path.stem}.csv"
        entries.append(reconstruct_one(coefficient_path, output_path))

    manifest = {
        "schema_version": 1,
        "photometry_model": PHOTOMETRY_MODEL,
        "gaiaxpy_version": GAIA_XPY_VERSION,
        "generation_timestamp_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "band_nm": [BAND_MIN_NM, BAND_MAX_NM],
        "grid_step_nm": GRID_STEP_NM,
        "entries": entries,
    }
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"reconstructed {len(entries)} sources -> {args.output_dir}")


if __name__ == "__main__":
    main()
