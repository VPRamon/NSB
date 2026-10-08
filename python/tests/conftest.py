# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
from datetime import datetime, timezone

import nsb
import pytest
import siderust


@pytest.fixture
def observer():
    # Matches the CTAO-S geometry fixture used by Rust end-to-end tests.
    return siderust.Observer(-70.31634444444444, -24.683427777777776, 2184.6)


@pytest.fixture
def direction():
    return siderust.Direction(266.41683, -29.00781)


@pytest.fixture
def evaluator():
    return nsb.NsbEvaluator()


@pytest.fixture
def point_time():
    return datetime(2023, 9, 4, 1, 48, tzinfo=timezone.utc)
