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
    assert full == pytest.approx(
        evaluator.evaluate(q).integrated_photons_cm2_ns_sr, rel=1e-12
    )
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


def test_band_integration_releases_the_gil():
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
            evaluator.integrate_band(q, 360, 570)
            if progressed.is_set():
                break
        # Observe progress before restoring normal Python thread switching.
        ran_during_rust = progressed.is_set()
    finally:
        sys.setswitchinterval(previous)
        thread.join(timeout=5)
    assert ran_during_rust
