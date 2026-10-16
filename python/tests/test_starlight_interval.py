# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""External candidate identity and explicit pixel observables survive PyO3."""
import hashlib
import json

import nsb
import pytest


def fixture():
    return {
        "schema": "nsb-starlight-interval-map-v1",
        "product_id": "starlight-gaia-dr3-xp-360-570-v1",
        "band_nm": [360, 570],
        "bin_edges_nm": [360, 390, 420, 450, 480, 510, 540, 570],
        "nside": 1, "ordering": "ring", "coordinate_frame": "galactic",
        "domain": "top-of-atmosphere", "units": "photon/(cm2 ns sr)",
        "wavelength_convention": "vacuum", "maturity": "experimental",
        "population_policy": "gaia-dr3-quality-xp-only-positive-subbands-v1",
        "non_xp_inference": None, "selection_correction": None,
        "bright_star_supplement": None, "systematic_uncertainty": None,
        "statistical_covariance": "full BP coefficient covariance; source-independent; bin cross covariance not packed",
        "design_sha256": "a" * 64,
        "input_partitions": {"fixture": {"shard_sha256": "b" * 64,
            "inputs": {"gaia-source": "c" * 64, "xp-continuous": "d" * 64}}},
        "catalogue_rows": 12, "xp_rows": 12, "unmatched_xp_rows": 0,
        "admitted_sources": 12, "excluded_sources": {},
        "pixels": [{"pixel": i, "sources": 1, "radiance": [1.0] * 7,
            "statistical_sigma": [0.1] * 7, "integrated_statistical_sigma": 0.3}
            for i in range(12)],
    }


def test_external_interval_identity_and_direct_transport(tmp_path):
    path = tmp_path / "candidate.json"
    data = json.dumps(fixture()).encode()
    path.write_bytes(data)
    sha = hashlib.sha256(data).hexdigest()
    product = nsb.StarlightMap.load(str(path), sha)
    assert product.sha256 == sha
    assert list(product.band_nm) == [360, 570]
    assert product.nside == 1
    assert product.admitted_sources == 12
    assert product.maturity == "experimental"
    assert product.top_of_atmosphere_pixel(0.0, 0.0) == (7.0, 0.3)
    mid, low, high, sigma = product.direct_pixel_bounds(0.0, 0.0, 30.0, 790.0)
    assert 0 < low < mid < high < 7.0
    assert sigma > 0
    with pytest.raises(nsb.InvalidMapError):
        nsb.StarlightMap.load(str(path), "0" * 64)
    with pytest.raises(nsb.OutOfRangeError):
        product.direct_pixel_bounds(0.0, 0.0, -1.0, 790.0)
    with pytest.raises(nsb.OutOfRangeError):
        product.direct_pixel_bounds(0.0, 0.0, 30.0, -1.0)
    with pytest.raises(nsb.OutOfRangeError):
        product.top_of_atmosphere_pixel(360.0, 0.0)

    clearer = product.direct_pixel_bounds(0.0, 0.0, 30.0, 790.0, aerosol_tau550=0.01)[0]
    hazier = product.direct_pixel_bounds(0.0, 0.0, 30.0, 790.0, aerosol_tau550=0.1)[0]
    assert hazier < mid < clearer
    with pytest.raises(nsb.OutOfRangeError):
        product.direct_pixel_bounds(0.0, 0.0, 30.0, 790.0, aerosol_tau550=-0.1)
    with pytest.raises(nsb.UnsupportedError):
        product.direct_pixel_bounds(0.0, 0.0, 30.0, 790.0, aerosol_power=1.0)


def load_candidate(tmp_path, candidate):
    path = tmp_path / "candidate.json"
    data = json.dumps(candidate).encode()
    path.write_bytes(data)
    return nsb.StarlightMap.load(str(path), hashlib.sha256(data).hexdigest())


@pytest.mark.parametrize(
    "channel", ["radiance", "statistical_sigma", "integrated_statistical_sigma"]
)
def test_empty_pixel_requires_zero_radiance_and_uncertainty(tmp_path, channel):
    candidate = fixture()
    candidate["pixels"][0] = {
        "pixel": 0, "sources": 0, "radiance": [0.0] * 7,
        "statistical_sigma": [0.0] * 7, "integrated_statistical_sigma": 0.0,
    }
    candidate["admitted_sources"] = 11
    candidate["excluded_sources"] = {"no_band_calibrated_non_xp_inference": 1}
    assert load_candidate(tmp_path, candidate).admitted_sources == 11

    if channel == "integrated_statistical_sigma":
        candidate["pixels"][0][channel] = 0.5
    else:
        candidate["pixels"][0][channel][0] = 0.5
    with pytest.raises(nsb.InvalidMapError):
        load_candidate(tmp_path, candidate)


@pytest.mark.parametrize(
    "field", [
        "non_xp_inference", "selection_correction",
        "bright_star_supplement", "systematic_uncertainty",
    ]
)
def test_explicit_null_provenance_required_in_python(tmp_path, field):
    candidate = fixture()
    del candidate[field]
    with pytest.raises(nsb.InvalidMapError):
        load_candidate(tmp_path, candidate)

    candidate = fixture()
    candidate[field] = {"unsupported": True}
    with pytest.raises(nsb.InvalidMapError):
        load_candidate(tmp_path, candidate)


@pytest.mark.parametrize(
    "edges", [[350, 570], [350, 410, 470, 570], list(range(350, 551, 20))]
)
def test_generic_starlight_map_supports_arbitrary_bins_without_new_python_types(
    tmp_path, edges
):
    bins = len(edges) - 1
    candidate = {
        "schema": "nsb-starlight-spectral-map-v2",
        "product_id": "synthetic-variable-bins",
        "band_nm": [edges[0], edges[-1]],
        "bin_edges_nm": edges,
        "nside": 1, "ordering": "ring",
        "coordinate_frame": "galactic", "domain": "top-of-atmosphere",
        "units": "photon/(cm2 ns sr)", "wavelength_convention": "vacuum",
        "maturity": "experimental",
        "provenance": {
            "source_catalogue": "synthetic",
            "source_license": "test-only",
            "source_selection": "all",
            "generation_method": "test fixture",
            "limitations": ["not scientific data"],
        },
        "catalogue_rows": 12, "admitted_sources": 12,
        "excluded_sources": {},
        "pixels": [
            {"pixel": i, "sources": 1, "radiance": [1.0] * bins,
             "statistical_sigma": [0.1] * bins,
             "integrated_statistical_sigma": 0.5}
            for i in range(12)
        ],
    }
    product = load_candidate(tmp_path, candidate)
    assert product.bin_edges_nm == edges
    assert list(product.band_nm) == [edges[0], edges[-1]]
    assert product.top_of_atmosphere_pixel(0.0, 0.0) == (float(bins), 0.5)
    assert product.integrate_band(0.0, 0.0, edges[0], edges[-1]) == (
        float(bins), 0.5
    )
    if bins > 2:
        assert product.integrate_band(0.0, 0.0, edges[0], edges[2]) == (
            2.0, None
        )
    with pytest.raises(nsb.UnsupportedError):
        product.integrate_band(0.0, 0.0, edges[0] + 1, edges[-1])
    with pytest.raises(nsb.UnsupportedError):
        product.integrate_band(0.0, 0.0, edges[0], edges[-1] + 1)

    candidate["pixels"] = [
        {"pixel": i, "sources": 1, "radiance": [1.0] * bins}
        for i in range(12)
    ]
    without_sigma = load_candidate(tmp_path, candidate)
    assert without_sigma.top_of_atmosphere_pixel(0.0, 0.0) == (
        float(bins), None
    )
    with pytest.raises(nsb.UnsupportedError):
        without_sigma.direct_pixel_bounds(0.0, 0.0, 30.0, 790.0)

    candidate["maturity"] = "production"
    with pytest.raises(nsb.InvalidMapError):
        load_candidate(tmp_path, candidate)
