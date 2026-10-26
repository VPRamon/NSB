# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Physical tests for the matched Starlight NSB↔nsb2 benchmark (#298)."""
from __future__ import annotations

import copy
import importlib.util
import json
import os
import subprocess
import sys
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
    importlib.util.find_spec("nsb") is None and not os.getenv("NSB_REQUIRE_CROSS_IMPLEMENTATION"),
    reason="nsb extension unavailable",
)
def test_level1_synthetic_starlight_map_matches_fixture():
    cases = study.level1_synthetic_nsb_cases()
    for case in cases:
        if case["case_id"] == "L1_synth_partial_bin_rejected":
            assert case["passed"] is True
            continue
        assert case["absolute_difference"] == pytest.approx(0.0, abs=1e-15)


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
    study.validate_artifact(artifact)
    for label, suffix in (("faint_only", "nsb2_faint_only"), ("faint_xhip", "nsb2_faint_xhip")):
        cases = [c for c in artifact["level2_cases"] if c["model_nsb2"] == suffix]
        # Independently reconstruct residuals; no frozen scientific scores.
        rel = [abs(c["nsb2"] - c["nsb"]) / abs(c["nsb"]) for c in cases]
        summary = artifact["aggregates"][f"level2_conditional_{label}"]
        assert summary["n_cases"] == len(study.SKY_DIRECTIONS)
        assert summary["median_relative_difference"] == pytest.approx(np.median(rel))
        assert summary["max_relative_difference"] == pytest.approx(max(rel))
    provenance = artifact["provenance"]
    assert len(provenance["nsb_checkout"]["input_manifest_sha256"]) == 64
    assert provenance["nsb_build"]["declared_source_revision"]
    assert provenance["nsb_build"]["binaries"]
    assert "nsb-py" in provenance["dependencies"]
    assert provenance["runner_sha256"] == study.sha256_file(SCRIPT)
    assert provenance["nsb_assets"]


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
    (importlib.util.find_spec("nsb") is None or importlib.util.find_spec("nsb2") is None)
    and not os.getenv("NSB_REQUIRE_CROSS_IMPLEMENTATION"),
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
        assert cases[case_id]["relative_difference"] == pytest.approx(0.0, abs=1e-12)


def test_comparability_matrix_includes_cross_implementation_equivalent():
    equiv = [row for row in study.COMPARABILITY_MATRIX if row["status"] == "equivalent"]
    assert any("NSB StarlightMap and nsb2" in row["comparison"] for row in equiv)


requires_nsb2 = pytest.mark.skipif(
    importlib.util.find_spec("nsb2") is None and not os.getenv("NSB_REQUIRE_CROSS_IMPLEMENTATION"),
    reason="optional nsb2 geometry/radiometry dependencies unavailable",
)


def synthetic_catalog(lon, lat, weights):
    import astropy.units as u
    from astropy.coordinates import SkyCoord
    from nsb2.core.sources import CatalogSource
    from nsb2.core.spectral import SpectralGrid

    wave = np.linspace(300, 650, 701) * u.nm
    # Two identical parameter nodes, a single photon spectral template.
    flux = np.ones((2, len(wave), 1)) / u.s / u.cm**2 / u.nm
    grid = SpectralGrid([np.array([0., 1.])], wave, flux)
    coords = SkyCoord(l=np.asarray(lon) * u.deg, b=np.asarray(lat) * u.deg, frame="galactic").icrs
    source = CatalogSource(coords, np.asarray(weights) * u.dimensionless_unscaled,
                           np.zeros(len(coords)), grid)
    source.build_balltree()
    return source


@requires_nsb2
def test_catalog_exact_pixel_selection_and_flux_normalization():
    import astropy.units as u
    import healpy as hp
    from astropy.coordinates import SkyCoord

    # Galactic nside=1 north-cap pixel centered on (45, asin(2/3)).
    # Source at (0, 40) lies outside the pixel but inside its circumscribed cone.
    source = synthetic_catalog([45, 44, 0, 225], [42, 43, 40, -42], [2, 3, 100, 1000])
    target = SkyCoord(l=45*u.deg, b=42*u.deg, frame="galactic").icrs
    field, pixel = study.catalog_pixel_field(source, target.ra.deg, target.dec.deg, nside=1)
    assert pixel == 0
    assert len(field.coords) == 2
    omega = 4*np.pi/12
    result = study._nsb2_band_integral(field, 300, 650, solid_angle_sr=omega)
    # Integral of (2+3) photons cm^-2 s^-1 nm^-1 over 350 nm, divided by true area.
    assert result == pytest.approx(5*350/omega*1e-9, rel=1e-12)
    assert study._nsb2_band_integral(field, 300, 650, solid_angle_sr=omega/4) == pytest.approx(4*result)
    with pytest.raises(ValueError, match="solid angle"):
        study._nsb2_band_integral(field, 300, 650)
    # Legacy resolution radius is sqrt(area), so its cone area is ~pi pixels.
    radius = hp.nside2resol(128)
    assert 2*np.pi*(1-np.cos(radius))/study.healpix_pixel_steradian(128) == pytest.approx(np.pi, rel=1e-5)


@requires_nsb2
def test_catalog_empty_pixel_is_zero_but_unavailable_source_fails():
    import astropy.units as u
    from astropy.coordinates import SkyCoord
    source = synthetic_catalog([225], [-42], [1])
    target = SkyCoord(l=45*u.deg, b=42*u.deg, frame="galactic").icrs
    field, _ = study.catalog_pixel_field(source, target.ra.deg, target.dec.deg, nside=1)
    assert len(field.coords) == 0
    assert study._nsb2_band_integral(field, 300, 650, solid_angle_sr=1) == 0
    source = synthetic_catalog([45, 46], [42, 42], [1, 1])
    source.data[1] = 2  # outside template parameter grid -> all templates unavailable
    field, _ = study.catalog_pixel_field(source, target.ra.deg, target.dec.deg, nside=1)
    with pytest.raises(study.UnavailableSpectralTemplate):
        study._nsb2_band_integral(field, 300, 650, solid_angle_sr=1)


@requires_nsb2
def test_diffuse_radiance_has_no_second_area_normalization():
    import astropy.units as u
    from nsb2.core.dtypes import SourceField
    source = synthetic_catalog([45], [42], [2])
    field = SourceField(source.coords, source.weight/u.sr, source.data, source.spectral_grid, True)
    assert study._nsb2_band_integral(field, 300, 650) == pytest.approx(2*350*1e-9)
    with pytest.raises(ValueError, match="Diffuse radiance"):
        study._nsb2_band_integral(field, 300, 650, solid_angle_sr=2)
    field.weights = field.weights * u.sr  # corrupted diffuse field with flux units
    with pytest.raises(u.UnitConversionError):
        study._nsb2_band_integral(field, 300, 650)


@requires_nsb2
def test_band_endpoints_and_row_accounting_fail_closed():
    import astropy.units as u
    from nsb2.core.dtypes import SourceField
    source = synthetic_catalog([45], [42], [1])
    field = SourceField(source.coords, source.weight/u.sr, source.data, source.spectral_grid, True)
    with pytest.raises(ValueError, match="endpoints"):
        study._nsb2_band_integral(field, 300.1, 650)
    with pytest.raises(ValueError, match="coverage"):
        study._nsb2_band_integral(field, 299, 650)
    field.spectral_data = np.zeros((2, 1))
    with pytest.raises(ValueError, match="selected-source rows"):
        study._nsb2_band_integral(field, 300, 650)


def test_selected_sources_with_zero_template_columns_fail_closed():
    with pytest.raises(study.UnavailableSpectralTemplate):
        study.reduce_pickles_template_rates(np.empty((2, 0)))


def load_committed_artifact():
    return json.loads(study.ARTIFACT.read_text())


def mock_build_inputs(monkeypatch):
    old = load_committed_artifact()
    monkeypatch.setattr(study, "require_nsb2_pin", lambda **k: old["provenance"]["nsb2_identity"])
    monkeypatch.setattr(study, "level1_analytic_operator_cases", lambda: old["level1_cases"])
    monkeypatch.setattr(study, "level1_synthetic_nsb_cases", list)
    monkeypatch.setattr(study, "level1_cross_implementation_constant_spectrum", list)
    monkeypatch.setattr(study, "population_audit", lambda **k: {})
    monkeypatch.setattr(study, "runtime_provenance", lambda *a: {})
    monkeypatch.setattr(study, "git_state", lambda *a: {})
    return old


def test_unexpected_level2_failure_propagates(monkeypatch):
    mock_build_inputs(monkeypatch)
    def failure(**kwargs):
        raise RuntimeError("synthetic Level-2 failure")
    monkeypatch.setattr(study, "level2_sky_direction_cases", failure)
    with pytest.raises(RuntimeError, match="synthetic Level-2 failure"):
        study.build_artifact(skip_level2=False, offline_audit=True,
                             include_bright_catalog=False, allow_unverified_nsb2=False)


def test_cli_failure_is_nonzero_and_preserves_existing_output(tmp_path):
    path = tmp_path / "valid.json"
    path.write_text("previous evidence")
    code = f"""import sys
sys.path.insert(0, {str(SCRIPT.parent)!r})
import matched_starlight_nsb2_benchmark as b
def failure(**kwargs):
    raise RuntimeError('synthetic Level-2 failure')
b.build_artifact = failure
sys.argv = ['benchmark', '--output', {str(path)!r}]
b.main()
"""
    result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=False)
    assert result.returncode != 0
    assert "synthetic Level-2 failure" in result.stderr
    assert path.read_text() == "previous evidence"


def test_explicit_level1_run_has_no_level2_or_closure_claim(monkeypatch, tmp_path):
    mock_build_inputs(monkeypatch)
    def forbidden(**kwargs):
        pytest.fail("Level-2 should not execute")
    monkeypatch.setattr(study, "level2_sky_direction_cases", forbidden)
    artifact = study.build_artifact(skip_level2=True, offline_audit=True,
                                    include_bright_catalog=False, allow_unverified_nsb2=False)
    assert artifact["execution"]["level2"] == "skipped_explicitly"
    assert not artifact["level2_cases"]
    assert not artifact["issue_298_closure"]["closes"]
    study.write_artifact(tmp_path / "partial.json", artifact)
    with pytest.raises(ValueError, match="Official artifact"):
        study.write_artifact(study.ARTIFACT, artifact)


@pytest.mark.parametrize("corrupt", ["missing_l1", "missing_l2", "duplicate_l2", "nonfinite", "wrong_metric", "failed_l1", "wrong_l1_relative"])
def test_incomplete_or_invalid_evidence_cannot_replace_output(tmp_path, corrupt):
    artifact = load_committed_artifact()
    if corrupt == "missing_l1":
        artifact["level1_cases"].pop()
    elif corrupt == "missing_l2":
        artifact["level2_cases"].pop()
    elif corrupt == "duplicate_l2":
        artifact["level2_cases"][-1] = copy.deepcopy(artifact["level2_cases"][0])
    elif corrupt == "nonfinite":
        artifact["level2_cases"][0]["nsb2"] = float("nan")
    elif corrupt == "wrong_metric":
        artifact["level2_cases"][0]["absolute_difference"] = 1000
    elif corrupt == "wrong_l1_relative":
        case = artifact["level1_cases"][0]
        case["nsb"] += 0.01
        delta = case["nsb"] - case["reference"]
        case["signed_difference"] = delta
        case["absolute_difference"] = abs(delta)
        case["relative_difference"] = 0.0
    else:
        artifact["level1_cases"][0]["relative_difference"] = 0.1
        artifact["level1_cases"][0]["absolute_difference"] = 0.1
    output = tmp_path / "valid.json"
    output.write_text("previous evidence")
    with pytest.raises(ValueError):
        study.write_artifact(output, artifact)
    assert output.read_text() == "previous evidence"


def test_atomic_replace_failure_preserves_output_and_cleans_temporary(monkeypatch, tmp_path):
    artifact = load_committed_artifact()
    path = tmp_path / "valid.json"
    path.write_text("previous evidence")
    def fail(*args):
        raise OSError("replace failed")
    monkeypatch.setattr(study.os, "replace", fail)
    with pytest.raises(OSError):
        study.write_artifact(path, artifact)
    assert path.read_text() == "previous evidence"
    assert list(tmp_path.iterdir()) == [path]


def test_git_provenance_distinguishes_staged_unstaged_and_untracked(tmp_path):
    def git(*args):
        return subprocess.check_output(["git", "-C", str(tmp_path), *args], text=True)
    git("init", "-q")
    git("config", "user.email", "fixture@example.org")
    git("config", "user.name", "Fixture")
    source = tmp_path / "source.py"
    source.write_text("one")
    git("add", ".")
    git("commit", "-qm", "initial")
    clean = study.git_state(tmp_path)
    source.write_text("two")
    git("add", "source.py")
    source.write_text("three")
    (tmp_path / "new.txt").write_text("untracked")
    dirty = study.git_state(tmp_path)
    assert clean["head"] == dirty["head"]
    assert clean["dirty"] is False and dirty["dirty"] is True
    assert "MM source.py" in dirty["status"] and "?? new.txt" in dirty["status"]
    assert clean["input_manifest_sha256"] != dirty["input_manifest_sha256"]
    generated = tmp_path / "docs/validation/artifacts/matched-starlight-nsb2-v1.json"
    generated.parent.mkdir(parents=True)
    generated.write_text("generated result")
    assert study.git_state(tmp_path)["input_manifest_sha256"] == dirty["input_manifest_sha256"]


def test_dirty_nsb2_checkout_refuses_pinned_reference(monkeypatch):
    monkeypatch.setattr(study, "resolve_nsb2_identity", lambda: {
        "verification": "dirty", "resolved_commit": study.NSB2_COMMIT})
    with pytest.raises(RuntimeError, match="dirty"):
        study.require_nsb2_pin()
    assert study.require_nsb2_pin(allow_unverified=True)["evidence_status"] == "provenance_limited_dirty"


@requires_nsb2
def test_actual_healpix_source_matches_uniform_catalog_pixel_mean():
    from types import SimpleNamespace

    import astropy.units as u
    import healpy as hp
    from astropy.coordinates import SkyCoord
    # Populate every native ICRS cell so colour interpolation has no missing neighbours.
    lon_i, lat_i = hp.pix2ang(1, np.arange(12), nest=True, lonlat=True)
    source = synthetic_catalog(lon_i, lat_i, np.full(12, 2.))
    source.coords = SkyCoord(ra=lon_i*u.deg, dec=lat_i*u.deg)
    source.frame = source.coords.frame
    mapped = source.to_map(1)
    field, _ = mapped.query_direct(SimpleNamespace(origin="icrs"), source.coords[:1], np.zeros(1))
    expected = 2*350/study.healpix_pixel_steradian(1)*1e-9
    assert study._nsb2_band_integral(field, 300, 650) == pytest.approx(expected, rel=1e-12)


def test_required_cross_implementation_dependencies_and_pin():
    if os.getenv("NSB_REQUIRE_CROSS_IMPLEMENTATION"):
        import nsb2  # noqa: F401

        import nsb  # noqa: F401
        assert study.require_nsb2_pin()["evidence_status"] == "pinned_reference"


def test_optional_bright_comparison_excludes_unverified_xhip_union(monkeypatch):
    monkeypatch.setattr(study, "load_nsb2_sources", lambda **k: {})
    monkeypatch.setattr(study, "nsb_toa_starlight", lambda *a: 1.)
    monkeypatch.setattr(study, "nsb2_toa_components", lambda *a: {
        "radiances": {"faint": 1., "bright": 2., "suppl": 100.},
        "catalog_counts": {"bright": 1, "suppl": 1},
        "nsb_galactic_ring_pixel": 0, "catalog_solid_angle_sr": 1.})
    cases = study.level2_sky_direction_cases(include_bright_catalog=True)
    assert len(cases) == 3*len(study.SKY_DIRECTIONS)
    assert all(c["nsb2"] == 3 for c in cases if c["model_nsb2"] == "nsb2_faint_bright")
    assert not any(c["model_nsb2"] == "nsb2_faint_bright_xhip" for c in cases)


def test_resolver_detects_dirty_nsb2_owned_checkout(monkeypatch, tmp_path):
    from types import SimpleNamespace
    monkeypatch.setitem(sys.modules, "nsb2", SimpleNamespace(__file__=str(tmp_path / "nsb2/__init__.py")))
    monkeypatch.setattr(study, "_git_commit_owned_by_nsb2", lambda p: (study.NSB2_COMMIT, f"git:{tmp_path}"))
    monkeypatch.setattr(study, "git_state", lambda p: {"head": study.NSB2_COMMIT, "dirty": True, "status": [" M nsb2/core/sources.py"]})
    identity = study.resolve_nsb2_identity()
    assert identity["resolved_commit"] == study.NSB2_COMMIT
    assert identity["verification"] == "dirty"
    assert identity["checkout"]["dirty"] is True
