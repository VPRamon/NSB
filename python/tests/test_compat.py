from datetime import datetime, timedelta, timezone

import nsb
import pytest


def test_observer_and_direction_bridge(observer, direction):
    assert observer.lon_deg == pytest.approx(-70.31634444444444)
    assert observer.lat_deg == pytest.approx(-24.683427777777776)
    assert observer.height_m == pytest.approx(2184.6)
    assert direction.ra_deg == pytest.approx(266.41683)
    assert direction.dec_deg == pytest.approx(-29.00781)


def test_aware_non_utc_datetime_is_normalized(observer, direction):
    local = datetime(2023, 9, 4, 3, 48, tzinfo=timezone(timedelta(hours=2)))
    query = nsb.PointQuery(observer, local, direction)
    expected = datetime(2023, 9, 4, 1, 48, tzinfo=timezone.utc)
    # tempoch's chrono round-trip contract is accurate to <50 microseconds.
    assert abs(query.time - expected) < timedelta(microseconds=50)


def test_naive_datetime_is_rejected_as_nsb_input_error(observer, direction):
    with pytest.raises(nsb.OutOfRangeError, match="timezone-aware"):
        nsb.PointQuery(observer, datetime(2023, 9, 4, 1, 48), direction)


@pytest.mark.parametrize("naive_endpoint", ["start", "end"])
def test_threshold_naive_datetimes_are_rejected_as_nsb_input_errors(
    observer, direction, naive_endpoint
):
    aware = datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc)
    start = datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc)
    end = aware
    if naive_endpoint == "start":
        start = start.replace(tzinfo=None)
    else:
        end = end.replace(tzinfo=None)

    with pytest.raises(nsb.OutOfRangeError, match=f"{naive_endpoint} must be timezone-aware"):
        nsb.ThresholdQuery(observer, direction, start, end, 1.0)


def test_non_finite_compat_values_are_rejected():
    with pytest.raises(nsb.OutOfRangeError, match="finite"):
        nsb.Direction(float("nan"), 0.0)
