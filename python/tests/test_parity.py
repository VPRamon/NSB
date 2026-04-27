"""Smoke test for the Rust↔Python bindings.

Requires ``maturin develop --features python`` to have been run first.
"""
import json
import os
from pathlib import Path

import nsb


GOLDEN = (
    Path(__file__).resolve().parents[2] / "tests" / "golden" /
    "ctaos_sgrA_2023-09-04_0148.json"
)


def test_parity_against_python_golden():
    with open(GOLDEN) as f:
        gold = json.load(f)

    r = nsb.calculate(
        gold["inputs"]["site"],
        gold["inputs"]["obstime"],
        gold["inputs"]["source"],
        ["zodiacal", "starlight", "airglow"],
    )

    assert abs(r.integrated - gold["totals"]["integrated"]) / gold["totals"]["integrated"] < 0.02
    assert abs(r.b_mag - gold["totals"]["b_mag"]) < 0.05
    assert abs(r.v_mag - gold["totals"]["v_mag"]) < 0.05
