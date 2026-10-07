# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
from datetime import datetime, timedelta, timezone
import sys
import threading

import nsb


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
