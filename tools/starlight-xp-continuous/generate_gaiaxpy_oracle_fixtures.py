#!/usr/bin/env python3
"""Generate frozen GaiaXPy oracle fixtures for Rust continuous calibrate parity tests."""

from __future__ import annotations

import argparse
import gzip
import json
from pathlib import Path

import gaiaxpy
import numpy as np
import pandas as pd

BAND_MIN_NM = 336.0
BAND_MAX_NM = 650.0
GRID_STEP_NM = 2.0


def sampling_grid() -> np.ndarray:
    return np.arange(BAND_MIN_NM, BAND_MAX_NM + GRID_STEP_NM * 0.5, GRID_STEP_NM)


def integrate_flux(sampling: np.ndarray, flux: np.ndarray, flux_error: np.ndarray) -> tuple[float, float]:
    photon_energy_j = 6.62607015e-34 * 299792458.0 / (sampling * 1e-9)
    trapz = getattr(np, "trapezoid", None) or getattr(np, "trapz")
    photon_flux = flux / photon_energy_j
    photon_unc = flux_error / photon_energy_j
    return float(trapz(photon_flux, sampling)), float(trapz(photon_unc, sampling))


def canonical_from_bulk_row(row: pd.Series) -> dict:
    def parse_array(value) -> list[float]:
        if isinstance(value, (list, tuple)):
            return [float(part) for part in value]
        text = str(value).strip()
        if text.startswith("[") and text.endswith("]"):
            text = text[1:-1]
        elif text.startswith("(") and text.endswith(")"):
            text = text[1:-1]
        return [float(part.strip()) for part in text.split(",") if part.strip()]

    return {
        "schema_version": 2,
        "source_id": str(int(row["source_id"])),
        "bp_n_parameters": int(row["bp_n_parameters"]),
        "rp_n_parameters": int(row["rp_n_parameters"]),
        "bp_n_relevant_bases": None,
        "rp_n_relevant_bases": None,
        "bp_standard_deviation": float(row["bp_standard_deviation"]),
        "rp_standard_deviation": float(row["rp_standard_deviation"]),
        "bp_coefficients": parse_array(row["bp_coefficients"]),
        "rp_coefficients": parse_array(row["rp_coefficients"]),
        "bp_coefficient_errors": parse_array(row["bp_coefficient_errors"]),
        "rp_coefficient_errors": parse_array(row["rp_coefficient_errors"]),
        "bp_coefficient_correlations": parse_array(row["bp_coefficient_correlations"]),
        "rp_coefficient_correlations": parse_array(row["rp_coefficient_correlations"]),
        "source_format": "bulk_ecsv",
        "source_checksum": None,
        "quality_flags": [],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bulk-gz", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=32)
    args = parser.parse_args()

    sampling = sampling_grid()
    records: list[dict] = []
    with gzip.open(args.bulk_gz, "rt", encoding="utf-8") as handle:
        frame = pd.read_csv(handle, comment="#", nrows=args.limit)
    for _, row in frame.iterrows():
        canonical = canonical_from_bulk_row(row)
        coeff_path = args.output.parent / f"oracle_{canonical['source_id']}.csv"
        coeff_path.parent.mkdir(parents=True, exist_ok=True)
        df = pd.DataFrame(
            [
                {
                    "source_id": canonical["source_id"],
                    "bp_n_parameters": canonical["bp_n_parameters"],
                    "bp_standard_deviation": canonical["bp_standard_deviation"],
                    "rp_n_parameters": canonical["rp_n_parameters"],
                    "rp_standard_deviation": canonical["rp_standard_deviation"],
                    "bp_coefficients": tuple(canonical["bp_coefficients"]),
                    "bp_coefficient_errors": tuple(canonical["bp_coefficient_errors"]),
                    "bp_coefficient_correlations": tuple(
                        canonical["bp_coefficient_correlations"]
                    ),
                    "rp_coefficients": tuple(canonical["rp_coefficients"]),
                    "rp_coefficient_errors": tuple(canonical["rp_coefficient_errors"]),
                    "rp_coefficient_correlations": tuple(
                        canonical["rp_coefficient_correlations"]
                    ),
                }
            ]
        )
        df.to_csv(coeff_path, index=False)
        calibrated, _ = gaiaxpy.calibrate(
            str(coeff_path),
            sampling=sampling,
            save_file=False,
            truncation=False,
        )
        row_cal = calibrated.iloc[0]
        flux = np.asarray(row_cal["flux"], dtype=float)
        flux_error = np.asarray(row_cal["flux_error"], dtype=float)
        integral, uncertainty = integrate_flux(sampling, flux, flux_error)
        records.append(
            {
                "canonical": canonical,
                "oracle": {
                    "flux_336_650_ph_m2_s": integral,
                    "statistical_uncertainty_336_650_ph_m2_s": uncertainty,
                },
            }
        )
        coeff_path.unlink(missing_ok=True)

    payload = {
        "schema_version": 1,
        "gaiaxpy_version": gaiaxpy.__version__,
        "bulk_gz": str(args.bulk_gz),
        "record_count": len(records),
        "records": records,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {args.output} ({len(records)} records)")


if __name__ == "__main__":
    main()
