# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
from datetime import datetime, timezone

import nsb
import pytest


def test_invalid_threshold_maps_to_structured_nsb_error(observer, direction):
    with pytest.raises(nsb.OutOfRangeError, match="threshold_photons_cm2_ns_sr must be non-negative"):
        nsb.ThresholdQuery(
            observer,
            direction,
            datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
            datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
            -1.0,
        )


def test_removed_sampling_parameter_is_rejected(observer, direction):
    with pytest.raises(TypeError, match="sample_step_s"):
        nsb.ThresholdQuery(
            observer,
            direction,
            datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
            datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
            1.0e6,
            sample_step_s=600.0,
        )
