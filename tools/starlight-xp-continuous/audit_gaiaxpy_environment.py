#!/usr/bin/env python3
"""Audit pinned GaiaXPy environment and calibration basis checksums."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import sys
from datetime import datetime, timezone
from importlib import metadata
from pathlib import Path

import gaiaxpy


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package_hash(dist_name: str) -> str:
    files = metadata.files(dist_name) or []
    digest = hashlib.sha256()
    for entry in sorted(files, key=lambda item: str(item)):
        path = Path(metadata.distribution(dist_name).locate_file(entry))
        if path.is_file():
            digest.update(path.name.encode())
            digest.update(sha256_file(path).encode())
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--output-sha256", type=Path, required=True)
    args = parser.parse_args()

    gaiaxpy_root = Path(gaiaxpy.__file__).resolve().parent
    config_dir = gaiaxpy_root / "config"
    calibration_files = {}
    for path in sorted(config_dir.glob("*.csv")):
        calibration_files[path.name] = {
            "path": str(path),
            "sha256": sha256_file(path),
            "size_bytes": path.stat().st_size,
        }

    deps = {}
    for req_str in sorted(set(metadata.requires("GaiaXPy") or [])):
        name = req_str.split("[", 1)[0].split(";", 1)[0].strip()
        for sep in ("==", ">=", "<=", "~=", "!=", ">", "<"):
            if sep in name:
                name = name.split(sep, 1)[0].strip()
                break
        try:
            deps[name] = metadata.version(name)
        except metadata.PackageNotFoundError:
            deps[name] = "missing"

    report = {
        "schema_version": 1,
        "generation_timestamp_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "python_version": sys.version,
        "platform": platform.platform(),
        "gaiaxpy_version": gaiaxpy.__version__,
        "gaiaxpy_package_hash": package_hash("GaiaXPy"),
        "gaiaxpy_install_path": str(gaiaxpy_root),
        "dependencies": deps,
        "calibration_data": {
            "location": str(config_dir),
            "basis_function_version": "bpC03/rpC03 (GaiaXPy 2.1.4 config CSV set)",
            "files": calibration_files,
        },
        "sampling_convention": {
            "grid_nm": [336.0, 650.0],
            "step_nm": 2.0,
            "inclusive_upper_nm": 650.0,
        },
        "input_coefficient_schema": "Gaia DR3 XP continuous mean spectrum (BP/RP coefficients)",
        "flux_units": "W m^-2 nm^-1 (GaiaXPy calibrate output)",
        "covariance_representation": "GaiaXPy calibrate optional correlation matrix (not persisted by default)",
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(report, indent=2) + "\n"
    args.output_json.write_text(payload, encoding="utf-8")
    args.output_sha256.write_text(f"{hashlib.sha256(payload.encode()).hexdigest()}\n")

    print(f"audited GaiaXPy {gaiaxpy.__version__}: {len(calibration_files)} calibration files")


if __name__ == "__main__":
    main()
