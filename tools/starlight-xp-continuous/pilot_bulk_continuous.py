#!/usr/bin/env python3
"""Deprecated wrapper: use Rust run_phase5b_mini_pilot for canonical bulk streaming."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bulk-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--row-limit", type=int, default=1000)
    parser.add_argument("--batch-size", type=int, default=100)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()

    bulk_files = sorted(args.bulk_dir.glob("XpContinuousMeanSpectrum_*.csv.gz"))
    if not bulk_files:
        raise SystemExit(f"no bulk files under {args.bulk_dir}")
    bulk_gz = bulk_files[0]
    repo_root = Path(__file__).resolve().parents[2]
    cmd = [
        "cargo",
        "run",
        "--locked",
        "-q",
        "-p",
        "nsb-data-tools",
        "--bin",
        "run_phase5b_mini_pilot",
        "--",
        "--bulk-gz",
        str(bulk_gz),
        "--output-dir",
        str(args.output_dir),
        "--row-limit",
        str(args.row_limit),
        "--batch-size",
        str(args.batch_size),
        "--python",
        str(repo_root / "tools/starlight-xp-continuous/.venv/bin/python"),
        "--reconstruct-script",
        str(repo_root / "tools/starlight-xp-continuous/reconstruct_and_integrate.py"),
    ]
    if args.resume:
        cmd.append("--resume")
    subprocess.run(cmd, cwd=repo_root, check=True)


if __name__ == "__main__":
    main()
