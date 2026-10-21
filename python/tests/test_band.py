# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Independent sub-band contracts; same scientific model as the public integral."""

from datetime import datetime, timezone
import math
import nsb
import pytest
import siderust


def query(components=None):
    return nsb.PointQuery(
        siderust.Observer(lon_deg=-70.4045, lat_deg=-24.6272, height_m=2635),
        datetime(2023, 9, 15, 5, tzinfo=timezone.utc),
        siderust.Direction(ra_deg=10, dec_deg=-20),
        components=components
        if components is not None
        else nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW,
    )


def test_full_band_matches_existing_integral_and_subbands_add():
    evaluator = nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky())
    q = query()
    full = evaluator.integrate_band(q, 300, 650)
    assert math.isfinite(full) and full > 0
    assert full == pytest.approx(evaluator.evaluate(q).integrated_photons_cm2_ns_sr, rel=1e-12)
    assert evaluator.integrate_band(q, 300, 450.25) + evaluator.integrate_band(
        q, 450.25, 650
    ) == pytest.approx(full, rel=1e-12)
    assert 0 < evaluator.integrate_band(q, 360, 570) < full


@pytest.mark.parametrize(
    "lower,upper", [(299, 650), (300, 651), (650, 300), (400, 400), (float("nan"), 550)]
)
def test_invalid_domain_errors(lower, upper):
    with pytest.raises(nsb.NsbError):
        nsb.NsbEvaluator().integrate_band(query(), lower, upper)


def test_integrated_starlight_cannot_be_reconstructed():
    with pytest.raises(nsb.UnsupportedError, match="starlight"):
        nsb.NsbEvaluator().integrate_band(query(nsb.ComponentMask.STARLIGHT), 360, 570)


@pytest.mark.parametrize("lower,upper", [(float("-inf"), 500), (400, float("inf"))])
def test_nonfinite_bounds_are_rejected(lower, upper):
    with pytest.raises(nsb.NsbError):
        nsb.NsbEvaluator().integrate_band(query(), lower, upper)


def test_empty_and_physical_zero_moon_remain_zero():
    evaluator = nsb.NsbEvaluator()
    for mask in [nsb.ComponentMask.ZODIACAL & nsb.ComponentMask.AIRGLOW, nsb.ComponentMask.MOON]:
        assert evaluator.integrate_band(query(mask), 360, 570) == 0


def test_nonzero_moon_has_no_invented_spectrum():
    q = nsb.PointQuery(
        siderust.Observer(lon_deg=-70.4045, lat_deg=-24.6272, height_m=2635),
        datetime(2023, 9, 29, 3, tzinfo=timezone.utc),
        siderust.Direction(ra_deg=10, dec_deg=-20),
        components=nsb.ComponentMask.MOON,
    )
    with pytest.raises(nsb.UnsupportedError, match="moonlight"):
        nsb.NsbEvaluator().integrate_band(q, 360, 570)


@pytest.mark.parametrize("method", ["integrate_band", "integrate_band_components"])
def test_band_integration_releases_the_gil(method):
    import sys
    import threading

    ready = threading.Event()
    started = threading.Event()
    progressed = threading.Event()

    def worker():
        ready.set()
        started.wait()
        progressed.set()

    thread = threading.Thread(target=worker)
    thread.start()
    assert ready.wait(timeout=5)
    evaluator = nsb.NsbEvaluator()
    q = query()
    previous = sys.getswitchinterval()
    sys.setswitchinterval(1000.0)
    try:
        started.set()
        for _ in range(64):
            getattr(evaluator, method)(q, 360, 570)
            if progressed.is_set():
                break
        # Observe progress before restoring normal Python thread switching.
        ran_during_rust = progressed.is_set()
    finally:
        sys.setswitchinterval(previous)
        thread.join(timeout=5)
    assert ran_during_rust


def test_component_band_result_parity_and_fail_closed():
    evaluator = nsb.NsbEvaluator()
    result = evaluator.integrate_band_components(query(), 360, 570)
    assert (result.lower_nm, result.upper_nm) == (360, 570)
    # Same fixed query and pre-refactor scalar reference as Rust band tests.
    assert result.integrated_photons_cm2_ns_sr == pytest.approx(
        0.10387771910979773, abs=1e-14, rel=0
    )
    assert result.integrated_photons_cm2_ns_sr == evaluator.integrate_band(query(), 360, 570)
    assert [c.name for c in result.components] == ["zodiacal", "airglow"]
    assert (
        sum(c.integrated_photons_cm2_ns_sr for c in result.components)
        == result.integrated_photons_cm2_ns_sr
    )
    for component in result.components:
        assert component.statistical_uncertainty is None
        assert component.systematic_uncertainty is None
        assert component.metadata.provenance
    assert result.components[1].metadata.solar_activity_provenance
    with pytest.raises(nsb.UnsupportedError):
        evaluator.integrate_band_components(query(nsb.ComponentMask.STARLIGHT), 360, 570)
    with pytest.raises(nsb.NsbError):
        evaluator.integrate_band_components(query(), 360, 651)


@pytest.mark.parametrize("mask", [0, 1, 4, 5, 8, 9, 12, 13])
def test_structured_components_single_mixed_empty_and_partitioned(mask):
    # Compose using the same public selectors callers use; no custom flag API.
    selected = nsb.ComponentMask.ZODIACAL & nsb.ComponentMask.AIRGLOW
    for bit, flag in (
        (1, nsb.ComponentMask.ZODIACAL),
        (4, nsb.ComponentMask.AIRGLOW),
        (8, nsb.ComponentMask.MOON),
    ):
        if mask & bit:
            selected = selected | flag
    q = query(selected)
    evaluator = nsb.NsbEvaluator()
    whole = evaluator.integrate_band_components(q, 360, 570)
    assert isinstance(whole, nsb.RestrictedBandResult)
    assert len(whole.components) == mask.bit_count()
    assert whole.integrated_photons_cm2_ns_sr == evaluator.integrate_band(q, 360, 570)
    parts = [evaluator.integrate_band_components(q, a, a + 30) for a in range(360, 570, 30)]
    assert sum(p.integrated_photons_cm2_ns_sr for p in parts) == pytest.approx(
        whole.integrated_photons_cm2_ns_sr, rel=1e-12, abs=0
    )
    full = evaluator.integrate_band_components(q, 300, 650)
    assert full.integrated_photons_cm2_ns_sr == pytest.approx(
        evaluator.evaluate(q).integrated_photons_cm2_ns_sr, rel=1e-12, abs=0
    )
    for i, component in enumerate(whole.components):
        assert isinstance(component, nsb.RestrictedBandComponent)
        assert component.statistical_uncertainty is component.systematic_uncertainty is None
        assert sum(p.components[i].integrated_photons_cm2_ns_sr for p in parts) == pytest.approx(
            component.integrated_photons_cm2_ns_sr, rel=1e-12, abs=0
        )
    with pytest.raises(AttributeError):
        whole.lower_nm = 400


@pytest.mark.parametrize(
    "lower,upper",
    [(299, 570), (360, 651), (500, 400), (400, 400), (float("nan"), 500), (360, float("inf"))],
)
def test_structured_band_rejects_invalid_wavelengths(lower, upper):
    with pytest.raises(nsb.OutOfRangeError):
        nsb.NsbEvaluator().integrate_band_components(query(), lower, upper)


def test_mixed_unsupported_and_nonzero_moon_fail_without_partial_results():
    evaluator = nsb.NsbEvaluator()
    with pytest.raises(nsb.UnsupportedError, match="starlight"):
        evaluator.integrate_band_components(
            query(nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.STARLIGHT), 360, 570
        )
    q = nsb.PointQuery(
        siderust.Observer(-70.4045, -24.6272, 2635),
        datetime(2023, 9, 29, 3, tzinfo=timezone.utc),
        siderust.Direction(10, -20),
        components=nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.MOON,
    )
    with pytest.raises(nsb.UnsupportedError, match="moonlight"):
        evaluator.integrate_band_components(q, 360, 570)


def test_unattenuated_zodiacal_provenance_and_resolved_solar_identity():
    config = nsb.NsbModelConfig.generic_clear_sky().with_zodiacal_extinction(
        nsb.ZodiacalExtinction.NONE
    )
    evaluator = nsb.NsbEvaluator(config)
    out = evaluator.integrate_band_components(query(), 360, 570)
    full = evaluator.evaluate(query())
    assert "atmospheric propagation none" in out.components[0].metadata.provenance
    assert out.components[0].metadata.provenance == full.components[0].metadata.provenance
    air = out.components[1].metadata
    assert air.solar_activity_provenance == full.components[1].metadata.solar_activity_provenance
    assert air.solar_activity_provenance in air.provenance
    assert air.airglow_selection_kind == full.components[1].metadata.airglow_selection_kind
    assert air.status == full.components[1].metadata.status
    assert air.airglow_physical_outcome is None
    assert out.integrated_photons_cm2_ns_sr == evaluator.integrate_band(query(), 360, 570)
