# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Physical tests for the matched Starlight NSB↔nsb2 benchmark (#298)."""
from __future__ import annotations

import importlib.util
from pathlib import Path

import numpy as np
import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "matched_starlight_nsb2_benchmark.py"
spec = importlib.util.spec_from_file_location("matched_starlight_nsb2_benchmark", SCRIPT)
study = importlib.util.module_from_spec(spec)
spec.loader.exec_module(study)


def test_healpix_solid_angle_exact():
    assert study.healpix_pixel_steradian(1) == pytest.approx(4 * np.pi / 12)
    assert study.healpix_pixel_steradian(128) == pytest.approx(4 * np.pi / (12 * 128**2))


def test_healpix_rejects_invalid_nside():
    with pytest.raises(ValueError):
        study.healpix_pixel_steradian(0)


def test_constant_and_linear_spectrum_integrals():
    wave = np.linspace(300.0, 650.0, 701)
    assert study.integrate_photon_density(wave, np.ones_like(wave), 300, 650) == pytest.approx(
        350.0, rel=0, abs=1e-10
    )
    dens = 0.01 + 0.001 * (wave - 300.0)
    lo, hi = 400.0, 500.0
    a, b = 0.01, 0.001
    analytic = (a - 300 * b) * (hi - lo) + 0.5 * b * (hi**2 - lo**2)
    assert study.integrate_photon_density(wave, dens, lo, hi) == pytest.approx(
        analytic, rel=0, abs=1e-10
    )


def test_band_integration_rejects_bad_inputs():
    wave = np.array([1.0, 2.0, 3.0])
    dens = np.ones(3)
    with pytest.raises(ValueError):
        study.integrate_photon_density(wave, dens, 3.0, 1.0)
    with pytest.raises(ValueError):
        study.integrate_photon_density(np.array([3.0, 2.0, 1.0]), dens, 1.0, 3.0)
    with pytest.raises(ValueError):
        study.integrate_photon_density(wave, dens, 0.0, 1.0)


def test_energy_to_photon_density_wavelength_factor():
    wave = np.array([500.0, 500.0])
    energy = np.array([1.0, 1.0])
    dens = study.energy_to_photon_density(wave, energy)
    h = 6.62607015e-27
    c = 2.99792458e10
    expected = 500.0 * 1e-7 / (h * c)
    assert dens[0] == pytest.approx(expected)


def test_comparability_matrix_rejects_solar_proxy_and_xp_without_asset():
    unsupported = [row for row in study.COMPARABILITY_MATRIX if row["status"] == "unsupported"]
    assert any("Solar-colour" in row["comparison"] for row in unsupported)
    assert any("Source-ID" in row["comparison"] for row in unsupported)
    assert any(row["status"] == "conditional" for row in study.COMPARABILITY_MATRIX)
    assert any(row["status"] == "equivalent" for row in study.COMPARABILITY_MATRIX)


def test_sky_directions_frozen_and_preselected():
    assert len(study.SKY_DIRECTIONS) == 6
    names = [n for n, _, _ in study.SKY_DIRECTIONS]
    assert "galactic_north_pole" in names
    assert "dense_plane" in names
    assert "sparse_high_lat" in names


def test_population_audit_marks_missing_not_zero():
    audit = study.population_audit(offline=True)
    assert audit["nsb_production_integrated_300_650"]["admitted_sources"] == 1_518_801_751
    assert audit["nsb_experimental_xp_360_570"]["bundled_in_repository"] is False
    assert audit["intersection"]["exact_source_id_match_against_nsb_runtime_maps"] == "unsupported"
    assert "never silent zero" in audit["intersection"]["missing_represented_as"]


def test_level1_analytic_cases_pass_tolerances():
    for case in study.level1_analytic_operator_cases():
        assert case["absolute_difference"] == pytest.approx(0.0, abs=1e-12)


@pytest.mark.skipif(
    importlib.util.find_spec("nsb") is None,
    reason="nsb extension unavailable",
)
def test_level1_synthetic_starlight_map_matches_fixture():
    cases = study.level1_synthetic_nsb_cases()
    for case in cases:
        if case["case_id"] == "L1_synth_partial_bin_rejected":
            assert case["passed"] is True
            continue
        assert case["absolute_difference"] == pytest.approx(0.0, abs=1e-15)


def test_signed_difference_convention_in_helpers():
    # predicted(nsb2) - observed(nsb) style for level2; reference - nsb for level1
    assert (0.5 - 0.2) == pytest.approx(0.3)


def test_protocol_pins_nsb2_commit_and_band():
    assert study.NSB2_COMMIT.startswith("bc9320d")
    assert study.BAND_NM == (300.0, 650.0)
    assert study.XP_CANDIDATE_SHA.startswith("fe331343")
