# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
from datetime import datetime, timezone

import nsb
import pytest


def test_invalid_threshold_maps_to_structured_nsb_error(observer, direction):
    with pytest.raises(nsb.OutOfRangeError, match="sample_step_s must be positive"):
        nsb.ThresholdQuery(
            observer,
            direction,
            datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
            datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
            1.0e6,
            sample_step_s=0.0,
        )
