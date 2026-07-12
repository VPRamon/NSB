#!/usr/bin/env python3
"""Export GaiaXPy 2.1.4 design matrices for the NSB 336-650 nm grid (Rust calibrate oracle)."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np

BAND_MIN_NM = 336.0
BAND_MAX_NM = 650.0
GRID_STEP_NM = 2.0


def sampling_grid() -> np.ndarray:
    return np.arange(BAND_MIN_NM, BAND_MAX_NM + GRID_STEP_NM * 0.5, GRID_STEP_NM)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    from gaiaxpy.calibrator.calibrator import __generate_xp_matrices_and_merge

    sampling = sampling_grid()
    design, merge = __generate_xp_matrices_and_merge(
        "calibrator", sampling, "v375wi", "v142r"
    )
    payload = {
        "schema_version": 1,
        "gaiaxpy_version": __import__("gaiaxpy").__version__,
        "bp_model": "v375wi",
        "rp_model": "v142r",
        "band_nm": [BAND_MIN_NM, BAND_MAX_NM],
        "grid_step_nm": GRID_STEP_NM,
        "sampling_nm": sampling.tolist(),
        "merge_bp": merge["bp"].tolist(),
        "merge_rp": merge["rp"].tolist(),
        "design_bp": design["bp"].get_design_matrix().tolist(),
        "design_rp": design["rp"].get_design_matrix().tolist(),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload) + "\n", encoding="utf-8")
    print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
