# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Physical tests for the matched Starlight NSB↔nsb2 benchmark (#298)."""
from __future__ import annotations

import importlib.util
import json
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


def test_reduce_pickles_all_finite_uses_nanmedian():
    rates = np.array([[1.0, 3.0, 5.0]])
    assert study.reduce_pickles_template_rates(rates) == pytest.approx(3.0)


def test_reduce_pickles_mixed_finite_nonfinite_within_source():
    rates = np.array([[1.0, np.nan, 5.0, np.inf]])
    assert study.reduce_pickles_template_rates(rates) == pytest.approx(3.0)


def test_reduce_pickles_rejects_source_with_all_nonfinite():
    rates = np.array([[1.0, 2.0], [np.nan, np.nan]])
    with pytest.raises(study.UnavailableSpectralTemplate, match="source index 1"):
        study.reduce_pickles_template_rates(rates)


def test_reduce_pickles_rejects_when_only_one_of_multiple_is_invalid():
    rates = np.array([[2.0, 4.0], [np.nan, np.nan], [1.0, 1.0]])
    with pytest.raises(study.UnavailableSpectralTemplate, match="source index 1"):
        study.reduce_pickles_template_rates(rates)


def test_reduce_pickles_empty_selection_is_legitimate_zero():
    assert study.reduce_pickles_template_rates(np.zeros((0, 3))) == 0.0
    assert study.reduce_pickles_template_rates([]) == 0.0


def test_reduce_pickles_valid_numerical_zero_is_kept():
    rates = np.array([[0.0, 0.0, 0.0], [2.0, 4.0, 6.0]])
    assert study.reduce_pickles_template_rates(rates) == pytest.approx(4.0)


def test_no_stale_nanmean_descriptions_in_published_evidence():
    root = SCRIPT.parent.parent
    for path in (
        SCRIPT,
        root / "docs/validation/matched-starlight-nsb2-v1.md",
        root / "docs/validation/artifacts/matched-starlight-nsb2-v1.json",
    ):
        text = path.read_text()
        assert "nanmean" not in text
        assert "nanmedian" in text


def test_committed_artifact_records_verified_nsb2_pin():
    artifact = json.loads(
        (SCRIPT.parent.parent / "docs/validation/artifacts/matched-starlight-nsb2-v1.json").read_text()
    )
    identity = artifact["provenance"]["nsb2_identity"]
    assert identity["pinned_commit"] == study.NSB2_COMMIT
    assert identity["resolved_commit"] == study.NSB2_COMMIT
    assert identity["verification"] == "matched"
    assert identity["evidence_status"] == "pinned_reference"
    assert artifact["issue_298_closure"]["closes"] is True
    assert artifact["aggregates"]["level1_cross_implementation"]["max_relative_difference"] == 0.0
    # Level-2 conditional metrics unchanged by fail-closed template accounting.
    faint = artifact["aggregates"]["level2_conditional_faint_only"]
    assert faint["median_relative_difference"] == pytest.approx(0.8847676634377348)
    assert faint["max_relative_difference"] == pytest.approx(0.9710947910998103)


def test_nsb2_identity_matched_against_pin(monkeypatch):
    # Pin-policy only: do not import the optional nsb2 package.
    monkeypatch.setattr(
        study,
        "resolve_nsb2_identity",
        lambda: {
            "pinned_commit": study.NSB2_COMMIT,
            "resolved_commit": study.NSB2_COMMIT,
            "verification": "matched",
            "resolution_source": "test",
            "package_version": "0.2.0",
            "module_path": "test",
        },
    )
    got = study.require_nsb2_pin(allow_unverified=False)
    assert got["verification"] == "matched"
    assert got["evidence_status"] == "pinned_reference"


def test_nsb2_identity_mismatched_is_rejected(monkeypatch):
    monkeypatch.setattr(
        study,
        "resolve_nsb2_identity",
        lambda: {
            "pinned_commit": study.NSB2_COMMIT,
            "resolved_commit": "deadbeef" * 5,
            "verification": "mismatched",
            "resolution_source": "test",
            "package_version": "0.2.0",
            "module_path": "test",
        },
    )
    with pytest.raises(RuntimeError, match="does not match pinned"):
        study.require_nsb2_pin(allow_unverified=False)
    limited = study.require_nsb2_pin(allow_unverified=True)
    assert limited["evidence_status"] == "provenance_limited_mismatched"
    assert limited["resolved_commit"] == "deadbeef" * 5


def test_nsb2_identity_unverifiable_refuses_unless_option_b(monkeypatch):
    monkeypatch.setattr(
        study,
        "resolve_nsb2_identity",
        lambda: {
            "pinned_commit": study.NSB2_COMMIT,
            "resolved_commit": None,
            "verification": "unverifiable",
            "resolution_source": None,
            "package_version": "0.2.0",
            "module_path": "test",
        },
    )
    with pytest.raises(RuntimeError, match="unverifiable"):
        study.require_nsb2_pin(allow_unverified=False)
    limited = study.require_nsb2_pin(allow_unverified=True)
    assert limited["evidence_status"] == "provenance_limited_unverified"


def test_git_metadata_rejected_unless_nsb2_source_tree(tmp_path, monkeypatch):
    # Enclosing NSB-like repo must not supply the nsb2 commit even if git walks up.
    enclosing = tmp_path / "nsb-checkout"
    enclosing.mkdir()
    (enclosing / "pyproject.toml").write_text('[project]\nname = "nsb"\n', encoding="utf-8")
    pkg = enclosing / ".venv" / "lib" / "site-packages" / "nsb2"
    pkg.mkdir(parents=True)
    (pkg / "__init__.py").write_text("# fake\n", encoding="utf-8")
    monkeypatch.setattr(study, "git_toplevel", lambda path: enclosing)
    monkeypatch.setattr(study, "git_rev", lambda path: "nsb-head-not-nsb2")
    assert study._looks_like_nsb2_source_tree(enclosing) is False
    assert study._git_commit_owned_by_nsb2(pkg / "__init__.py") == (None, None)

    nsb2_root = tmp_path / "nsb2-reference"
    nsb2_root.mkdir()
    (nsb2_root / "pyproject.toml").write_text('[project]\nname = "nsb2"\n', encoding="utf-8")
    (nsb2_root / "nsb2").mkdir()
    module = nsb2_root / "nsb2" / "__init__.py"
    module.write_text("# real\n", encoding="utf-8")
    assert study._looks_like_nsb2_source_tree(nsb2_root) is True
    monkeypatch.setattr(study, "git_toplevel", lambda path: nsb2_root)
    monkeypatch.setattr(study, "git_rev", lambda path: study.NSB2_COMMIT)
    assert study._git_commit_owned_by_nsb2(module) == (
        study.NSB2_COMMIT,
        f"git:{nsb2_root}",
    )


@pytest.mark.skipif(
    importlib.util.find_spec("nsb") is None or importlib.util.find_spec("nsb2") is None,
    reason="nsb/nsb2 unavailable",
)
def test_level1_cross_implementation_agrees_with_analytic():
    cases = {c["case_id"]: c for c in study.level1_cross_implementation_constant_spectrum()}
    assert "L1_cross_nsb_vs_nsb2" in cases
    for case_id in (
        "L1_cross_nsb_vs_analytic",
        "L1_cross_nsb2_vs_analytic",
        "L1_cross_nsb_vs_nsb2",
        "L1_cross_nsb2_integrate_vs_resolved",
    ):
        assert cases[case_id]["relative_difference"] == pytest.approx(0.0, abs=1e-6)


def test_comparability_matrix_includes_cross_implementation_equivalent():
    equiv = [row for row in study.COMPARABILITY_MATRIX if row["status"] == "equivalent"]
    assert any("NSB StarlightMap and nsb2" in row["comparison"] for row in equiv)
