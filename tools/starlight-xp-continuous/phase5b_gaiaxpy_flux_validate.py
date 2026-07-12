#!/usr/bin/env python3
"""Migration-only GaiaXPy spectral parity for bulk/DataLink canonical pairs.

This oracle compares calibrated samples only. Integrated photon flux and
uncertainty are validated by the authoritative Rust implementation.
"""

from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path

import gaiaxpy
import numpy as np

CONTRACT_PATH = (
    Path(__file__).resolve().parents[2]
    / "crates/nsb-data-tools/contracts/gaia_xp_photon_integration_v1.json"
)


def load_contract() -> dict:
    contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
    if contract.get("schema_version") != 1:
        raise RuntimeError("unsupported Gaia XP scientific contract schema")
    return contract


def sampling_grid(contract: dict) -> np.ndarray:
    band = contract["band"]
    grid = contract["sampled_grid"]
    count = grid["band_end_index"] - grid["band_start_index"] + 1
    sampling = band["min_nm"] + np.arange(count, dtype=float) * grid["step_nm"]
    if not np.isclose(sampling[-1], band["max_nm"]):
        raise RuntimeError("generated contract has inconsistent Gaia XP band grid")
    return sampling


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


def reconstruct(path: Path, sampling: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    calibrated, _ = gaiaxpy.calibrate(
        str(path), sampling=sampling, save_file=False, truncation=False
    )
    if len(calibrated) != 1:
        raise RuntimeError(f"expected one calibrated row for {path}, found {len(calibrated)}")
    row = calibrated.iloc[0]
    flux = np.asarray(row["flux"], dtype=float)
    flux_error = np.asarray(row["flux_error"], dtype=float)
    if flux.shape != sampling.shape or flux_error.shape != sampling.shape:
        raise RuntimeError(f"calibrated grid mismatch for {path}")
    return flux, flux_error


def relative_max(left: np.ndarray, right: np.ndarray, floor: float) -> float:
    denominator = np.maximum(np.maximum(np.abs(left), np.abs(right)), floor)
    return float(np.max(np.abs(left - right) / denominator))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gaiaxpy-csv-dir", type=Path, required=True)
    parser.add_argument("--comparison-json", type=Path, required=True)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--output-csv", type=Path, required=True)
    parser.add_argument("--inspect-json", type=Path, required=True)
    args = parser.parse_args()

    contract = load_contract()
    tolerance = contract["parity_tolerances"]["spectral_flux_relative"]
    floor = contract["parity_tolerances"]["absolute_floor"]
    comparison = json.loads(args.comparison_json.read_text(encoding="utf-8"))
    sampling = sampling_grid(contract)
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
        datalink_flux, datalink_unc = reconstruct(datalink_path, sampling)
        flux_relative_max = relative_max(bulk_flux, datalink_flux, floor)
        uncertainty_relative_max = relative_max(bulk_unc, datalink_unc, floor)
        equivalent = (
            flux_relative_max <= tolerance and uncertainty_relative_max <= tolerance
        )
        entry.update(
            {
                "spectral_flux_relative_max": flux_relative_max,
                "spectral_uncertainty_relative_max": uncertainty_relative_max,
                "gaiaxpy_equivalent": equivalent,
                "integration_owner": contract["integration"]["owner"],
                "status": (
                    "equivalent"
                    if equivalent and entry.get("canonical_equivalent")
                    else "spectral_mismatch"
                ),
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
    print(f"GaiaXPy spectral parity: {passed}/{len(enriched)} equivalent -> {args.output_json}")


if __name__ == "__main__":
    main()
