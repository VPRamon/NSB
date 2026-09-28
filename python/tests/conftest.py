from datetime import datetime, timezone

import nsb
import pytest


@pytest.fixture
def observer():
    # Matches the CTAO-S geometry fixture used by Rust end-to-end tests.
    return nsb.Observer(-70.31634444444444, -24.683427777777776, 2184.6)


@pytest.fixture
def direction():
    return nsb.Direction(266.41683, -29.00781)


@pytest.fixture
def evaluator():
    return nsb.NsbEvaluator()


@pytest.fixture
def point_time():
    return datetime(2023, 9, 4, 1, 48, tzinfo=timezone.utc)
