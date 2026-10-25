# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Physical and numerical tests for spectral-window diagnostics (#296)."""
from __future__ import annotations

import importlib.util
from pathlib import Path

import numpy as np
import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "spectral_window_diagnostics.py"
spec = importlib.util.spec_from_file_location("spectral_window_diagnostics", SCRIPT)
study = importlib.util.module_from_spec(spec)
spec.loader.exec_module(study)


# ---------------------------------------------------------------------------
# A. Wavelength conversion
# ---------------------------------------------------------------------------

def test_morton2000_places_palace_green_near_air_5577():
    air = study.vacuum_to_air_nm(557.8887)
    # NIST/IAU air wavelength of [OI] 5577 is ~557.7339 nm.
    assert air == pytest.approx(557.7339, abs=2e-3)


def test_morton2000_red_lines_and_round_trip():
    for vac in (630.2046, 636.5536, 639.35):
        air = study.vacuum_to_air_nm(vac)
        assert study.air_to_vacuum_nm(air) == pytest.approx(vac, abs=1e-7)
        assert air < vac
        # Optical vac−air offset is ~0.15–0.18 nm in this domain.
        assert 0.14 < (vac - air) < 0.20


def test_morton2000_rejects_unsupported_domain():
    with pytest.raises(ValueError, match="unsupported"):
        study.refractive_index_air_morton2000(1000.0)
    with pytest.raises(ValueError, match="Unsupported"):
        study.air_to_vacuum_nm(50.0)


def test_frozen_windows_use_converted_air_bounds():
    windows = study.oxygen_feature_windows()
    green = windows["oi_green"]
    assert green["vacuum_center_nm"] == 557.8887
    assert green["air_center_nm"] == pytest.approx(study.vacuum_to_air_nm(557.8887))
    assert green["air_bounds_nm"][0] == pytest.approx(
        study.vacuum_to_air_nm(557.8887 - 2.5)
    )
    # Combined red includes all three Or vacuum lines.
    combined = windows["oi_red_combined"]
    for line in (630.2046, 636.5536, 639.35):
        lo, hi = combined["vacuum_bounds_nm"]
        assert lo <= line < hi


# ---------------------------------------------------------------------------
# B. Band integration
# ---------------------------------------------------------------------------

def test_constant_continuum_integral_is_analytic():
    wave_nm = np.linspace(400.0, 500.0, 1001)
    dens = np.full_like(wave_nm, 0.02)
    got = study.integrate_density_nm(wave_nm, dens, 410.0, 430.0)
    assert got == pytest.approx(0.02 * 20.0, rel=0, abs=1e-12)


def test_linear_continuum_partial_window():
    wave_nm = np.linspace(500.0, 520.0, 401)
    dens = 0.01 + 0.001 * (wave_nm - 500.0)
    lo, hi = 505.0, 515.0
    # ∫(a + b(λ-500)) dλ from 505 to 515.
    a, b = 0.01, 0.001
    analytic = (a - 500 * b) * (hi - lo) + 0.5 * b * (hi**2 - lo**2)
    assert study.integrate_density_nm(wave_nm, dens, lo, hi) == pytest.approx(
        analytic, rel=0, abs=1e-10
    )


def test_narrow_line_entirely_inside_window_conserves_flux():
    wave_nm = np.linspace(550.0, 570.0, 2001)
    dens = study.convolve_line_flux(wave_nm, 557.7, line_flux=0.05, fwhm_nm=1.3)
    # Wide enough window: recover nearly all flux.
    got = study.integrate_density_nm(wave_nm, dens, 550.0, 570.0)
    assert got == pytest.approx(0.05, rel=0, abs=1e-4)


def test_line_near_boundary_and_outside_window():
    wave_nm = np.linspace(550.0, 570.0, 4001)
    dens = study.convolve_line_flux(wave_nm, 560.0, 0.04, fwhm_nm=1.0)
    inside = study.integrate_density_nm(wave_nm, dens, 557.5, 562.5)
    outside = study.integrate_density_nm(wave_nm, dens, 550.0, 555.0)
    assert inside > 0.035
    assert outside < 0.002


def test_blended_lines_add_under_wide_integral():
    wave_nm = np.linspace(620.0, 650.0, 6001)
    dens = (
        study.convolve_line_flux(wave_nm, 630.0, 0.03, 1.3)
        + study.convolve_line_flux(wave_nm, 636.4, 0.01, 1.3)
    )
    got = study.integrate_density_nm(wave_nm, dens, 620.0, 650.0)
    assert got == pytest.approx(0.04, rel=0, abs=2e-4)


# ---------------------------------------------------------------------------
# C. Instrumental LSF
# ---------------------------------------------------------------------------

def test_normalized_lsf_integrates_to_one():
    wave = np.linspace(540.0, 580.0, 8001)
    kernel = study.gaussian_lsf_kernel(wave, 557.7, 1.34)
    assert np.trapezoid(kernel, wave) == pytest.approx(1.0, abs=1e-6)


def test_finite_window_leakage_at_fors_like_fwhm():
    """±2.5 nm around a 1.34 nm FWHM Gaussian retains most of the line flux."""
    wave = np.linspace(540.0, 580.0, 16001)
    dens = study.convolve_line_flux(wave, 557.7, 1.0, fwhm_nm=1.34)
    captured = study.integrate_density_nm(wave, dens, 557.7 - 2.5, 557.7 + 2.5)
    assert captured > 0.99
    assert (1.0 - captured) < 0.01


def test_broader_lsf_increases_finite_window_leakage():
    wave = np.linspace(540.0, 580.0, 16001)
    narrow = study.integrate_density_nm(
        wave, study.convolve_line_flux(wave, 557.7, 1.0, 1.0), 555.2, 560.2
    )
    broad = study.integrate_density_nm(
        wave, study.convolve_line_flux(wave, 557.7, 1.0, 3.0), 555.2, 560.2
    )
    assert narrow > broad


def test_invalid_fwhm_rejected():
    wave = np.linspace(550.0, 560.0, 101)
    with pytest.raises(ValueError, match="FWHM"):
        study.gaussian_lsf_kernel(wave, 555.0, 0.0)
    with pytest.raises(ValueError, match="FWHM"):
        study.gaussian_lsf_kernel(wave, 555.0, -1.0)


# ---------------------------------------------------------------------------
# D. Physical units and continuum subtraction
# ---------------------------------------------------------------------------

def test_photon_density_conversion_factor():
    # 1 erg/s/cm2/A/arcsec2 at 5000 A (two samples to satisfy the grid contract).
    dens = study.photon_density_per_nm(
        np.array([5000.0, 5001.0]), np.array([1.0, 1.0])
    )[0]
    expected = 5000.0 * 1e-8 / study.HC_ERG_CM * study.ARCSEC2_PER_SR * 1e-8
    assert dens == pytest.approx(expected, rel=0, abs=0)


def test_continuum_subtraction_recovers_known_line_excess():
    # Build an energy-flux spectrum whose photon density is continuum + Gaussian line.
    wave_nm = np.linspace(540.0, 580.0, 4001)
    wave_a = wave_nm * 10.0
    continuum = 0.01 + 1e-4 * (wave_nm - 560.0)
    line = study.convolve_line_flux(wave_nm, 557.7, 0.02, fwhm_nm=1.2)
    dens = continuum + line
    # Invert photon_density: energy = dens / (wave_A * factors)
    factor = wave_a * 1e-8 / study.HC_ERG_CM * study.ARCSEC2_PER_SR * 1e-8
    energy = dens / factor
    result = study.continuum_subtracted_excess(wave_a, energy, air_center_nm=557.7)
    assert result["excess"] == pytest.approx(0.02, rel=0, abs=5e-4)


def test_line_outside_feature_window_gives_near_zero_excess():
    wave_nm = np.linspace(540.0, 580.0, 4001)
    wave_a = wave_nm * 10.0
    dens = 0.01 + study.convolve_line_flux(wave_nm, 570.0, 0.03, 1.2)
    factor = wave_a * 1e-8 / study.HC_ERG_CM * study.ARCSEC2_PER_SR * 1e-8
    energy = dens / factor
    result = study.continuum_subtracted_excess(wave_a, energy, air_center_nm=557.7)
    assert abs(result["excess"]) < 5e-4


def test_no_double_bin_width_in_density_integral():
    wave_nm = np.array([100.0, 101.0, 102.0])
    dens = np.array([2.0, 2.0, 2.0])
    # Integral over 1 nm of density 2 is 2, not 2*dλ².
    assert study.integrate_density_nm(wave_nm, dens, 100.5, 101.5) == pytest.approx(2.0)


# ---------------------------------------------------------------------------
# E. Statistics and integrity
# ---------------------------------------------------------------------------

def test_signed_residual_convention_and_mae_bias():
    resid = study.signed_residuals([1.0, 2.0, 3.0], [1.5, 1.5, 4.0])
    assert list(resid) == pytest.approx([0.5, -0.5, 1.0])
    summary = study.mae_bias(resid)
    assert summary["mae"] == pytest.approx(2.0 / 3.0)
    assert summary["bias"] == pytest.approx(1.0 / 3.0)
    assert summary["n"] == 3


def test_night_cluster_ci_is_deterministic():
    values = np.array([0.1, 0.2, 0.15, 0.25, 0.12, 0.22])
    nights = np.array([1, 1, 2, 2, 3, 3])
    a = study.night_cluster_ci(values, nights, np.mean, repetitions=200, seed=182)
    b = study.night_cluster_ci(values, nights, np.mean, repetitions=200, seed=182)
    assert a == b


def test_observation_alignment_rejects_length_mismatch():
    with pytest.raises(ValueError, match="mismatch"):
        study.signed_residuals([1.0, 2.0], [1.0])


def test_comparability_matrix_rejects_full_spectral_mae():
    unsupported = [row for row in study.COMPARABILITY_MATRIX if row["status"] == "unsupported"]
    assert any("spectral MAE" in row["quantity"] for row in unsupported)
    assert any("nsb2" in row["comparison"].lower() for row in unsupported)


def test_continuum_control_windows_are_marked_unsupported():
    windows = study.oxygen_feature_windows()
    assert windows["cont_blue_400_430"]["comparability"] == "unsupported_as_airglow_residual"
    assert windows["cont_mid_450_500"]["comparability"] == "unsupported_as_airglow_residual"


def test_protocol_constants_match_oxygen_documentation():
    assert study.HALF_WIDTH_NM == 2.5
    assert study.SIDEBAND_INNER_NM == 3.0
    assert study.SIDEBAND_OUTER_NM == 5.0
    assert study.SEED == 182
    assert study.BOOTSTRAP == 2000


def test_frozen_notebook_registration_and_measurement_parity():
    """Only tests inspect the notebook; the runner has no dynamic dependency."""
    import ast
    import json
    from scipy.integrate import simpson

    notebook = json.loads((SCRIPT.parents[1] / "examples/observational-validation.ipynb").read_text())
    names = {"SITE", "PATAT_SHA", "SOLAR_SHA", "COHORT_SHA", "CALIBRATION_NIGHTS",
             "HC_ERG_CM", "ARCSEC2_PER_SR", "exclusion", "measured_radiance"}
    nodes = []
    for cell in notebook["cells"]:
        if cell["cell_type"] != "code":
            continue
        for node in ast.parse("".join(cell["source"])).body:
            name = (node.name if isinstance(node, ast.FunctionDef) else
                    node.targets[0].id if isinstance(node, ast.Assign)
                    and isinstance(node.targets[0], ast.Name) else None)
            if name in names:
                nodes.append(node)
    assert len(nodes) == len(names), "Audit notebook registration changes explicitly"
    scope = {"np": np, "simpson": simpson}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), "notebook-parity", "exec"), scope)
    for name in names - {"exclusion", "measured_radiance"}:
        assert scope[name] == getattr(study, name)
    # Freeze sequential cuts, including their exact boundary comparisons.
    runner = ast.parse(SCRIPT.read_text())
    cut = next(n for n in runner.body if isinstance(n, ast.FunctionDef) and n.name == "exclusion")
    original = next(n for n in nodes if isinstance(n, ast.FunctionDef) and n.name == "exclusion")
    assert ast.dump(cut) == ast.dump(original)
    wave = np.array([3600., 3660., 4000., 5577., 6495., 6510.])
    flux = np.array([1., -0.1, 2., 1., 3., 1.]) * 1e-17
    for lower, upper in [(365., 650.), (400., 430.), (450., 500.)]:
        assert study.measured_band_integral(wave, flux, lower, upper) == scope["measured_radiance"](
            wave, flux, lower, upper)


def test_runner_does_not_read_notebook_or_construct_partial_models():
    import ast

    tree = ast.parse(SCRIPT.read_text())
    assert not any(isinstance(n, ast.Call) and isinstance(n.func, ast.Name)
                   and n.func.id in {"exec", "compile"} for n in ast.walk(tree))
    assert "notebook_namespace" not in study.__dict__
    assert "Models" not in study.__dict__


def test_night_cluster_ci_matches_independent_unequal_cluster_oracle():
    values = np.array([1., 2., 4., 10.])
    nights = np.array([1, 1, 2, 3])
    rng = np.random.default_rng(182)
    estimates = []
    for _ in range(300):
        draw = rng.choice(np.unique(nights), size=3, replace=True)
        indices = np.concatenate([np.flatnonzero(nights == night) for night in draw])
        estimates.append(np.mean(values[indices]))
    expected = np.quantile(estimates, [.025, .975])
    assert study.night_cluster_ci(values, nights, np.mean, 300) == pytest.approx(expected)


def test_continuum_controls_never_emit_residual_metrics():
    import json

    artifact = json.loads((SCRIPT.parents[1] / "docs/validation/artifacts/spectral-window-diagnostics-v1.json").read_text())
    for name in study.CONTINUUM_CONTROL_AIR_NM:
        entry = artifact["per_window"][name]
        assert entry["metrics_omitted_reason"] == "unsupported_as_airglow_residual"
        assert "native_operational" not in entry
        assert "matched_centered_f27_diagnostic" not in entry
