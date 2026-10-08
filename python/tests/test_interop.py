# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
from datetime import datetime, timedelta, timezone
from importlib import metadata

import nsb
import pytest
import siderust


def test_distribution_declares_compatible_siderust_runtime_dependency():
    requirements = metadata.requires("nsb-rust") or []
    siderust_requirements = [
        requirement
        for requirement in requirements
        if requirement.split(";", 1)[0].strip().startswith("siderust")
    ]

    assert len(siderust_requirements) == 1
    requirement = siderust_requirements[0].replace(" ", "")
    assert ">=0.2.2" in requirement
    assert "<0.3" in requirement
    assert metadata.version("siderust") == siderust.__version__


def test_point_query_uses_canonical_siderust_types(observer, direction, point_time):
    query = nsb.PointQuery(observer, point_time, direction)

    assert type(query.observer) is siderust.Observer
    assert type(query.target) is siderust.Direction
    assert not hasattr(nsb, "Observer")
    assert not hasattr(nsb, "Direction")


def test_threshold_query_uses_canonical_siderust_types(observer, direction):
    query = nsb.ThresholdQuery(
        observer,
        direction,
        datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
        datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
        1.0,
    )

    assert type(query.observer) is siderust.Observer
    assert type(query.target) is siderust.Direction
    assert query.start.tzinfo is timezone.utc
    assert query.end.tzinfo is timezone.utc


def test_direction_round_trip_preserves_icrs_coordinates(observer, direction, point_time):
    recovered = nsb.PointQuery(observer, point_time, direction).target

    assert recovered.ra_deg == pytest.approx(direction.ra_deg, abs=1.0e-12)
    assert recovered.dec_deg == pytest.approx(direction.dec_deg, abs=1.0e-12)


@pytest.mark.parametrize(
    ("observer_value", "direction_value"),
    [
        (object(), siderust.Direction(10.0, 20.0)),
        (siderust.Observer(0.0, 0.0), object()),
        (siderust.Direction(10.0, 20.0), siderust.Direction(10.0, 20.0)),
        (siderust.Observer(0.0, 0.0), siderust.Observer(0.0, 0.0)),
    ],
)
def test_wrong_siderust_object_types_are_rejected(
    observer_value, direction_value, point_time
):
    with pytest.raises(TypeError):
        nsb.PointQuery(observer_value, point_time, direction_value)


@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
@pytest.mark.parametrize("field", ["lon_deg", "lat_deg", "height_m"])
def test_siderust_rejects_non_finite_observer_parts(value, field):
    parts = {"lon_deg": 0.0, "lat_deg": 0.0, "height_m": 0.0}
    parts[field] = value

    with pytest.raises(ValueError, match=rf"{field} must be finite"):
        siderust.Observer(**parts)


@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
@pytest.mark.parametrize("field", ["ra_deg", "dec_deg"])
def test_siderust_rejects_non_finite_direction_parts(value, field):
    parts = {"ra_deg": 0.0, "dec_deg": 0.0}
    parts[field] = value

    with pytest.raises(ValueError, match=rf"{field} must be finite"):
        siderust.Direction(**parts)


def test_aware_non_utc_datetime_is_normalized(observer, direction):
    local = datetime(2023, 9, 4, 3, 48, tzinfo=timezone(timedelta(hours=2)))
    query = nsb.PointQuery(observer, local, direction)
    expected = datetime(2023, 9, 4, 1, 48, tzinfo=timezone.utc)
    assert abs(query.time - expected) < timedelta(microseconds=50)
    assert query.time.tzinfo is timezone.utc


def test_naive_datetime_is_rejected_as_nsb_input_error(observer, direction):
    with pytest.raises(nsb.OutOfRangeError, match=r"time:.*timezone-aware"):
        nsb.PointQuery(observer, datetime(2023, 9, 4, 1, 48), direction)


@pytest.mark.parametrize("naive_endpoint", ["start", "end"])
def test_threshold_naive_datetimes_are_rejected_as_nsb_input_errors(
    observer, direction, naive_endpoint
):
    start = datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc)
    end = datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc)
    if naive_endpoint == "start":
        start = start.replace(tzinfo=None)
    else:
        end = end.replace(tzinfo=None)

    with pytest.raises(
        nsb.OutOfRangeError, match=rf"{naive_endpoint}:.*timezone-aware"
    ):
        nsb.ThresholdQuery(observer, direction, start, end, 1.0)
