"""Scientific consistency tests for CTAO notebook helpers."""

from __future__ import annotations

from pathlib import Path

import pytest

from nsb_ctao.oi_photometer import assess_abastumani_equivalence, parse_iqsy_hourly
from nsb_ctao.rayleigh import (
    PHOTONS_CM2_NS_SR_PER_RAYLEIGH,
    photons_per_ns_sr_from_rayleigh,
    rayleigh_from_photons_per_ns_sr,
)

CTAO_ROOT = Path(__file__).resolve().parents[1]
ASSETS = CTAO_ROOT / "assets"


def test_rayleigh_roundtrip() -> None:
    r = 250.0
    ph = photons_per_ns_sr_from_rayleigh(r)
    assert abs(ph - r * PHOTONS_CM2_NS_SR_PER_RAYLEIGH) < 1e-18
    assert abs(rayleigh_from_photons_per_ns_sr(ph) - r) < 1e-9


def test_abastumani_fixtures_parse() -> None:
    path = ASSETS / "abas5577.int"
    if not path.is_file():
        pytest.skip("Abastumani fixture not packaged")
    rows = parse_iqsy_hourly(path, wavelength=5577)
    assert len(rows) > 100
    gate = assess_abastumani_equivalence(path, ASSETS / "abas6300.int")
    assert gate["admission"] == "REJECTED_FOR_ABSOLUTE_DEFAULT_VALIDATION"


@pytest.mark.skipif(
    __import__("importlib").util.find_spec("nsb") is None,
    reason="nsb not installed",
)
def test_physical_checks_pass() -> None:
    from nsb_ctao.physical_checks import run_physical_checks

    result = run_physical_checks()
    assert result["all_passed"]
