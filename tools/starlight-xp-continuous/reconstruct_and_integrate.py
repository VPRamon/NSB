#!/usr/bin/env python3
"""Migration-only GaiaXPy reconstruction of normalized XP continuous spectra.

Rust is the sole production owner of 336–650 nm photon-flux integration. This
script consumes the generated Rust scientific contract, calibrates spectra with
the frozen GaiaXPy oracle, and writes normalized samples for Rust validation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
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
    if contract.get("contract_id") != "gaia_dr3_xp_photon_integration_v1":
        raise RuntimeError("unexpected Gaia XP scientific contract id")
    return contract


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def sampling_grid(contract: dict) -> np.ndarray:
    band = contract["band"]
    grid = contract["sampled_grid"]
    count = grid["band_end_index"] - grid["band_start_index"] + 1
    sampling = band["min_nm"] + np.arange(count, dtype=float) * grid["step_nm"]
    if not np.isclose(sampling[-1], band["max_nm"]):
        raise RuntimeError("generated contract has inconsistent Gaia XP band grid")
    return sampling


def format_series(values: np.ndarray, scientific: bool) -> str:
    parts = []
    for value in values:
        if not np.isfinite(value):
            raise ValueError("non-finite calibrated flux sample")
        parts.append(f"{float(value):.8e}" if scientific else f"{float(value):.8f}")
    return ";".join(parts)


def write_normalized_csv(
    output_path: Path,
    source_id: str,
    wavelengths_nm: np.ndarray,
    flux_w_m2_nm: np.ndarray,
    flux_error_w_m2_nm: np.ndarray,
    contract: dict,
) -> None:
    columns = contract["identifiers"]
    part = output_path.with_suffix(output_path.suffix + ".part")
    header = (
        "source_id,"
        f"{columns['wavelength_column']},"
        f"{columns['flux_column']},"
        f"{columns['flux_error_column']}\n"
    )
    row = (
        f"{source_id},"
        f"{format_series(wavelengths_nm, False)},"
        f"{format_series(flux_w_m2_nm, True)},"
        f"{format_series(flux_error_w_m2_nm, True)}\n"
    )
    part.write_text(header + row, encoding="utf-8")
    part.replace(output_path)


def source_id_from_stem(stem: str) -> str:
    return stem.removeprefix("xp_source_")


def reconstruct_file(coefficient_path: Path, output_dir: Path, contract: dict) -> list[dict]:
    sampling = sampling_grid(contract)
    calibrated, _correlation = gaiaxpy.calibrate(
        str(coefficient_path), sampling=sampling, save_file=False, truncation=False
    )
    entries = []
    for _, row in calibrated.iterrows():
        source_id = str(int(row["source_id"]))
        output_path = output_dir / f"{source_id}.csv"
        if output_path.exists():
            entries.append(
                {
                    "source_id": source_id,
                    "status": "skipped_existing",
                    "output_checksum": sha256_file(output_path),
                }
            )
            continue
        flux = np.asarray(row["flux"], dtype=float)
        flux_error = np.asarray(row["flux_error"], dtype=float)
        if flux.shape != sampling.shape or flux_error.shape != sampling.shape:
            raise RuntimeError(f"calibrated grid mismatch for {source_id}")
        write_normalized_csv(output_path, source_id, sampling, flux, flux_error, contract)
        entries.append(
            {
                "source_id": source_id,
                "status": "reconstructed",
                "output_path": str(output_path),
                "coefficient_checksum": sha256_file(coefficient_path),
                "output_checksum": sha256_file(output_path),
                "samples": int(len(sampling)),
                "integration_status": "deferred_to_rust",
            }
        )
    return entries


def reconstruct_one(
    coefficient_path: Path, output_path: Path, contract: dict
) -> dict:
    if output_path.exists():
        return {
            "source_id": source_id_from_stem(coefficient_path.stem),
            "status": "skipped_existing",
            "output_checksum": sha256_file(output_path),
        }
    entries = reconstruct_file(coefficient_path, output_path.parent, contract)
    for entry in entries:
        if entry["source_id"] == source_id_from_stem(coefficient_path.stem):
            return entry
    raise RuntimeError(f"no calibrated row for {coefficient_path}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coefficients-dir", type=Path, default=None)
    parser.add_argument("--coefficient-file", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=None)
    args = parser.parse_args()

    contract = load_contract()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    entries = []
    if args.coefficient_file is not None:
        entries.extend(reconstruct_file(args.coefficient_file, args.output_dir, contract))
    else:
        if args.coefficients_dir is None:
            raise SystemExit("either --coefficients-dir or --coefficient-file is required")
        coefficient_paths = sorted(args.coefficients_dir.glob("*.csv"))
        if args.limit is not None:
            coefficient_paths = coefficient_paths[: args.limit]
        for coefficient_path in coefficient_paths:
            source_id = source_id_from_stem(coefficient_path.stem)
            output_path = args.output_dir / f"{source_id}.csv"
            entries.append(reconstruct_one(coefficient_path, output_path, contract))

    manifest = {
        "schema_version": 2,
        "scientific_contract_id": contract["contract_id"],
        "scientific_contract_schema_version": contract["schema_version"],
        "scientific_contract_checksum": sha256_file(CONTRACT_PATH),
        "photometry_model": contract["identifiers"]["continuous_photometry_model"],
        "gaiaxpy_version": gaiaxpy.__version__,
        "generation_timestamp_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "integration_owner": contract["integration"]["owner"],
        "entries": entries,
    }
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    part = args.manifest.with_suffix(args.manifest.suffix + ".part")
    part.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    part.replace(args.manifest)
    print(f"reconstructed {len(entries)} spectra for Rust integration -> {args.output_dir}")


if __name__ == "__main__":
    main()
