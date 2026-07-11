#!/usr/bin/env python3
"""Validate GaiaXPy 336–650 nm flux equivalence for bulk vs DataLink canonical CSV pairs."""

from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path

import gaiaxpy
import numpy as np

BAND_MIN_NM = 336.0
BAND_MAX_NM = 650.0
GRID_STEP_NM = 2.0
FLUX_RTOL = 1.0e-8
UNCERTAINTY_RTOL = 1.0e-6


def sampling_grid() -> np.ndarray:
    return np.arange(BAND_MIN_NM, BAND_MAX_NM + GRID_STEP_NM * 0.5, GRID_STEP_NM)


def inspect_table(path: Path) -> list[dict]:
    import pandas as pd

    table = pd.read_csv(path, comment="#")
    rows = []
    for column in table.columns:
        value = table[column].iloc[0]
        array_length = None
        shape = None
        if isinstance(value, str):
            stripped = value.strip()
            if stripped.startswith("(") and stripped.endswith(")"):
                array_length = len(stripped[1:-1].split(","))
                shape = (array_length,)
        rows.append(
            {
                "column": column,
                "dtype": str(table[column].dtype),
                "shape": shape,
                "array_length": array_length,
                "first_row_type": type(value).__name__,
            }
        )
    return rows


def _trapz(y: np.ndarray, x: np.ndarray) -> float:
    trapz = getattr(np, "trapezoid", None) or getattr(np, "trapz")
    return float(trapz(y, x))


def integrate_flux_ph_m2_s(flux_w_m2_nm: np.ndarray, wavelengths_nm: np.ndarray) -> float:
    photon_energy_j = 6.62607015e-34 * 299792458.0 / (wavelengths_nm * 1e-9)
    photon_flux = flux_w_m2_nm / photon_energy_j
    return _trapz(photon_flux, wavelengths_nm)


def integrate_uncertainty_ph_m2_s(
    flux_error_w_m2_nm: np.ndarray, wavelengths_nm: np.ndarray
) -> float:
    photon_energy_j = 6.62607015e-34 * 299792458.0 / (wavelengths_nm * 1e-9)
    photon_unc = flux_error_w_m2_nm / photon_energy_j
    return _trapz(photon_unc, wavelengths_nm)


def reconstruct(path: Path, sampling: np.ndarray) -> tuple[float, float]:
    calibrated, _ = gaiaxpy.calibrate(
        str(path),
        sampling=sampling,
        save_file=False,
        truncation=False,
    )
    if len(calibrated) != 1:
        raise RuntimeError(f"expected one calibrated row for {path}, found {len(calibrated)}")
    row = calibrated.iloc[0]
    flux = np.asarray(row["flux"], dtype=float)
    flux_error = np.asarray(row["flux_error"], dtype=float)
    integral = integrate_flux_ph_m2_s(flux, sampling)
    uncertainty = integrate_uncertainty_ph_m2_s(flux_error, sampling)
    return integral, uncertainty


def relative_diff(left: float, right: float) -> float:
    denom = max(abs(left), abs(right), 1.0e-30)
    return abs(left - right) / denom


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gaiaxpy-csv-dir", type=Path, required=True)
    parser.add_argument("--comparison-json", type=Path, required=True)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--output-csv", type=Path, required=True)
    parser.add_argument("--inspect-json", type=Path, required=True)
    args = parser.parse_args()

    comparison = json.loads(args.comparison_json.read_text(encoding="utf-8"))
    sampling = sampling_grid()
    inspect_rows: list[dict] = []
    enriched: list[dict] = []

    for row in comparison:
        source_id = row["source_id"]
        bulk_path = args.gaiaxpy_csv_dir / f"{source_id}_bulk.csv"
        datalink_path = args.gaiaxpy_csv_dir / f"{source_id}_datalink.csv"
        entry = dict(row)
        if not bulk_path.is_file() or not datalink_path.is_file():
            entry["status"] = "missing_gaiaxpy_csv"
            entry["gaiaxpy_equivalent"] = False
            enriched.append(entry)
            continue

        if not inspect_rows:
            inspect_rows.extend(inspect_table(bulk_path))

        bulk_flux, bulk_unc = reconstruct(bulk_path, sampling)
        dl_flux, dl_unc = reconstruct(datalink_path, sampling)
        flux_rel = relative_diff(bulk_flux, dl_flux)
        unc_rel = relative_diff(bulk_unc, dl_unc)
        gaiaxpy_ok = flux_rel <= FLUX_RTOL and unc_rel <= UNCERTAINTY_RTOL
        entry.update(
            {
                "reconstructed_flux_bulk": bulk_flux,
                "reconstructed_flux_datalink": dl_flux,
                "absolute_flux_diff": abs(bulk_flux - dl_flux),
                "relative_flux_diff": flux_rel,
                "uncertainty_bulk": bulk_unc,
                "uncertainty_datalink": dl_unc,
                "relative_uncertainty_diff": unc_rel,
                "gaiaxpy_equivalent": gaiaxpy_ok,
                "status": "equivalent" if gaiaxpy_ok and entry.get("canonical_equivalent") else "flux_mismatch",
            }
        )
        enriched.append(entry)

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(enriched, indent=2) + "\n", encoding="utf-8")
    args.inspect_json.write_text(json.dumps(inspect_rows, indent=2) + "\n", encoding="utf-8")

    fieldnames = list(enriched[0].keys()) if enriched else []
    with args.output_csv.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(enriched)

    passed = sum(1 for row in enriched if row.get("gaiaxpy_equivalent"))
    print(
        f"phase5b GaiaXPy flux validation: {passed}/{len(enriched)} equivalent -> {args.output_json}"
    )


if __name__ == "__main__":
    main()
