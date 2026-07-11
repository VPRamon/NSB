#!/usr/bin/env python3
"""Pilot: stream official XP continuous bulk files through GaiaXPy and integrate 336–650 nm."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import resource
import time
from datetime import datetime, timezone
from io import TextIOWrapper
from pathlib import Path

import gaiaxpy
import numpy as np
import pandas as pd

BAND_MIN_NM = 336.0
BAND_MAX_NM = 650.0
GRID_STEP_NM = 2.0
PHOTOMETRY_MODEL = "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sampling_grid() -> np.ndarray:
    return np.arange(BAND_MIN_NM, BAND_MAX_NM + GRID_STEP_NM * 0.5, GRID_STEP_NM)


def read_ecsv_table(path: Path) -> pd.DataFrame:
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rb") as raw:
        text = TextIOWrapper(raw, encoding="utf-8", errors="replace")
        return pd.read_csv(text, comment="#")


def coefficient_frame(rows: pd.DataFrame) -> pd.DataFrame:
    """Pass through bulk rows with all GaiaXPy-required metadata columns."""
    required = ["source_id", "bp_coefficients", "rp_coefficients"]
    missing = [name for name in required if name not in rows.columns]
    if missing:
        raise RuntimeError(f"bulk file missing columns: {missing}")
    return rows.copy()


def integrate_flux_w_m2_nm(flux_w_m2_nm: np.ndarray, wavelengths_nm: np.ndarray) -> float:
    photon_energy_j = 6.62607015e-34 * 299792458.0 / (wavelengths_nm * 1e-9)
    photon_flux = flux_w_m2_nm / photon_energy_j
    return float(np.trapz(photon_flux, wavelengths_nm))


def process_batch(
    coefficient_rows: pd.DataFrame,
    sampling: np.ndarray,
    checkpoint_path: Path,
    processed_ids: set[str],
) -> tuple[list[dict], float, int]:
    entries: list[dict] = []
    batch_bytes = 0
    started = time.perf_counter()
    calibrated, _ = gaiaxpy.calibrate(
        coefficient_rows,
        sampling=sampling,
        save_file=False,
        truncation=False,
    )
    elapsed = time.perf_counter() - started
    for _, row in calibrated.iterrows():
        source_id = str(int(row["source_id"]))
        if source_id in processed_ids:
            continue
        flux = np.asarray(row["flux"], dtype=float)
        if flux.shape != sampling.shape:
            raise RuntimeError(f"grid mismatch for {source_id}")
        integral = integrate_flux_w_m2_nm(flux, sampling)
        entry = {
            "source_id": source_id,
            "flux_336_650_ph_m2_s": integral,
            "samples": int(len(sampling)),
            "status": "reconstructed",
        }
        entries.append(entry)
        processed_ids.add(source_id)
        batch_bytes += 512
        with checkpoint_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry) + "\n")
    return entries, elapsed, batch_bytes


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bulk-dir", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--report-json", type=Path, required=True)
    parser.add_argument("--file-limit", type=int, default=None)
    parser.add_argument("--row-limit", type=int, default=256)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()

    bulk_files = sorted(args.bulk_dir.glob("XpContinuousMeanSpectrum_*.csv.gz"))
    if args.file_limit is not None:
        bulk_files = bulk_files[: args.file_limit]
    if not bulk_files:
        raise SystemExit(f"no bulk files under {args.bulk_dir}")

    processed_ids: set[str] = set()
    if args.resume and args.checkpoint.exists():
        for line in args.checkpoint.read_text(encoding="utf-8").splitlines():
            if line.strip():
                processed_ids.add(str(json.loads(line)["source_id"]))

    sampling = sampling_grid()
    peak_rss = 0
    total_sources = 0
    total_bytes = 0
    wall_started = time.perf_counter()
    file_reports: list[dict] = []

    for bulk_path in bulk_files:
        file_started = time.perf_counter()
        table = read_ecsv_table(bulk_path)
        if args.row_limit is not None:
            table = table.head(args.row_limit)
        coeffs = coefficient_frame(table)
        entries, batch_elapsed, batch_bytes = process_batch(
            coeffs, sampling, args.checkpoint, processed_ids
        )
        total_sources += len(entries)
        total_bytes += batch_bytes
        peak_rss = max(peak_rss, resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
        file_reports.append(
            {
                "filename": bulk_path.name,
                "sha256": sha256_file(bulk_path),
                "rows_requested": int(len(coeffs)),
                "sources_reconstructed": len(entries),
                "elapsed_seconds": batch_elapsed,
                "sources_per_second": len(entries) / max(batch_elapsed, 1e-6),
            }
        )

    wall_elapsed = time.perf_counter() - wall_started
    report = {
        "schema_version": 1,
        "photometry_model": PHOTOMETRY_MODEL,
        "gaiaxpy_version": gaiaxpy.__version__,
        "generation_timestamp_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "band_nm": [BAND_MIN_NM, BAND_MAX_NM],
        "bulk_files_processed": len(file_reports),
        "sources_reconstructed_total": total_sources,
        "sources_resumed_skipped": len(processed_ids) - total_sources,
        "wall_elapsed_seconds": wall_elapsed,
        "sources_per_second": total_sources / max(wall_elapsed, 1e-6),
        "peak_rss_kib": peak_rss,
        "estimated_full_population_seconds": 184_729_270 / max(total_sources / wall_elapsed, 1e-6),
        "files": file_reports,
    }
    args.report_json.parent.mkdir(parents=True, exist_ok=True)
    args.report_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"pilot reconstructed {total_sources} sources from {len(file_reports)} bulk files "
        f"at {report['sources_per_second']:.2f} sources/s -> {args.report_json}"
    )


if __name__ == "__main__":
    main()
