# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
from datetime import datetime, timedelta, timezone
import sys
import threading

import nsb
import pytest


def planning_query(observer, direction):
    return nsb.ThresholdQuery(
        observer,
        direction,
        datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
        datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
        1.0e6,
        components=nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW,
        sample_step_s=600.0,
        sun_altitude_ceiling_deg=None,
        target_altitude_floor_deg=None,
    )


def test_threshold_search_filters_and_context_reuse(observer, direction, evaluator):
    query = planning_query(observer, direction)
    direct = evaluator.periods_below_threshold(query)
    context = evaluator.prepare_site_window_context(query)
    reused = evaluator.periods_below_threshold_with_context(context, query)

    assert direct.periods == reused.periods
    assert len(direct.periods) == 1
    start, end = direct.periods[0]
    tolerance = timedelta(microseconds=50)
    assert abs(start - datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc)) < tolerance
    assert abs(end - datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc)) < tolerance
    assert start.tzinfo is timezone.utc


def test_certified_search_never_returns_an_incomplete_schedule(observer, direction, evaluator):
    query = planning_query(observer, direction)
    context = evaluator.prepare_site_window_context(query)
    for call in (
        lambda: evaluator.periods_below_threshold_certified(query),
        lambda: evaluator.periods_below_threshold_certified_with_context(context, query),
    ):
        with pytest.raises(nsb.UnsupportedError, match="incomplete certified threshold search"):
            call()


def test_certified_empty_component_sum_needs_no_sampling_configuration(observer, direction, evaluator):
    start = datetime(2023, 9, 4, 1, tzinfo=timezone.utc)
    end = start + timedelta(hours=1)
    query = nsb.ThresholdQuery(observer, direction, start, end, 0.0,
        components=nsb.ComponentMask.ZODIACAL & nsb.ComponentMask.MOON, sun_altitude_ceiling_deg=None,
        target_altitude_floor_deg=None)
    result = evaluator.periods_below_threshold_certified(query)
    context = evaluator.prepare_site_window_context(query)
    assert result.periods == evaluator.periods_below_threshold_certified_with_context(context, query).periods
    assert len(result.periods) == 1
    assert abs(result.periods[0][0] - start) < timedelta(microseconds=50)
    assert abs(result.periods[0][1] - end) < timedelta(microseconds=50)


def test_rust_heavy_planning_detaches_from_python(observer, direction, evaluator):
    query = nsb.ThresholdQuery(
        observer,
        direction,
        datetime(2023, 9, 5, 0, 0, tzinfo=timezone.utc),
        datetime(2023, 9, 5, 6, 0, tzinfo=timezone.utc),
        1.0e6,
        components=nsb.ComponentMask.AIRGLOW,
        sample_step_s=60.0,
        sun_altitude_ceiling_deg=None,
        target_altitude_floor_deg=None,
    )
    started = threading.Event()
    progressed = threading.Event()

    def worker():
        started.wait()
        progressed.set()

    thread = threading.Thread(target=worker)
    thread.start()
    previous = sys.getswitchinterval()
    sys.setswitchinterval(1000.0)
    try:
        started.set()
        evaluator.periods_below_threshold(query)
    finally:
        sys.setswitchinterval(previous)
        thread.join()

    assert progressed.is_set()


def test_adaptive_models_context_and_inward_datetime_precision(observer, direction, evaluator):
    query = planning_query(observer, direction)
    direct = evaluator.periods_below_threshold_adaptive(query)
    assert direct.periods == evaluator.periods_below_threshold(query).periods
    context = evaluator.prepare_site_window_context(query)
    assert direct.periods == evaluator.periods_below_threshold_adaptive_with_context(context, query).periods
    assert direct.periods == evaluator.periods_below_threshold_with_context(context, query).periods
    assert len(direct.periods) == 1
    assert direct.periods[0][0] >= query.start
    assert direct.periods[0][1] <= query.end
    assert direct.periods[0][0].tzinfo is timezone.utc


def test_adaptive_supports_both_lunar_models(observer):
    import siderust
    direction = siderust.Direction(83.6331, 22.0145)
    for model in (nsb.MoonlightModel.JONES2013_SPECTRAL, nsb.MoonlightModel.KRISCIUNAS_SCHAEFER1991):
        evaluator = nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky().with_moonlight_model(model))
        query = nsb.ThresholdQuery(observer, direction,
            datetime(2026, 1, 3, 1, tzinfo=timezone.utc),
            datetime(2026, 1, 3, 6, tzinfo=timezone.utc), 1.0e6,
            components=nsb.ComponentMask.ALL, sun_altitude_ceiling_deg=None,
            target_altitude_floor_deg=20.0)
        assert evaluator.periods_below_threshold_adaptive(query).periods


def test_joint_band_checks_minimum_at_exported_datetime_boundaries(observer, direction, evaluator):
    import siderust
    start = datetime(2023, 9, 4, 0, tzinfo=timezone.utc)
    end = start + timedelta(hours=6)
    target = siderust.Direction(266.41683, -29.00781)
    mask = nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW
    values = [evaluator.evaluate(nsb.PointQuery(observer, start + timedelta(minutes=i * 10), target, components=mask)).integrated_photons_cm2_ns_sr for i in range(37)]
    values.sort()
    minimum, maximum = values[len(values)//3], values[2*len(values)//3]
    query = nsb.ThresholdQuery(observer, target, start, end, maximum, components=mask,
        sun_altitude_ceiling_deg=None, target_altitude_floor_deg=None)
    context = evaluator.prepare_site_window_context(query)
    out = evaluator.periods_in_band_adaptive_with_context(context, query, minimum)
    assert out.periods == evaluator.periods_in_band_adaptive(query, minimum).periods
    assert out.periods
    for a, b in out.periods:
        for i in range(101):
            t = a + (b-a) * i / 100
            value = evaluator.evaluate(nsb.PointQuery(observer, t, target, components=mask)).integrated_photons_cm2_ns_sr
            assert minimum <= value <= maximum


def test_joint_band_equality_and_certified_semantics(observer, direction, evaluator):
    start = datetime(2023, 9, 4, 1, tzinfo=timezone.utc)
    query = nsb.ThresholdQuery(observer, direction, start, start+timedelta(hours=1), 0.0,
        components=nsb.ComponentMask.ZODIACAL & nsb.ComponentMask.MOON,
        sun_altitude_ceiling_deg=None, target_altitude_floor_deg=None)
    context = evaluator.prepare_site_window_context(query)
    assert len(evaluator.periods_in_band_adaptive(query, 0.0).periods) == 1
    assert len(evaluator.periods_in_band_certified_with_context(context, query, 0.0).periods) == 1
    for minimum in (-1.0, float('nan'), 1.0):
        with pytest.raises(nsb.OutOfRangeError):
            evaluator.periods_in_band_adaptive(query, minimum)
    dynamic = planning_query(observer, direction)
    context = evaluator.prepare_site_window_context(dynamic)
    with pytest.raises(nsb.UnsupportedError, match='certificate'):
        evaluator.periods_in_band_certified_with_context(context, dynamic, 0.0)
