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
    product = nsb.StarlightIntervalMap.load(str(path), sha)
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
        nsb.StarlightIntervalMap.load(str(path), "0" * 64)
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
