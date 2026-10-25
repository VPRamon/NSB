# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Pure contract checks for the offline solar-cadence reproduction runner."""
from datetime import date, timedelta
import importlib.util
from pathlib import Path

import numpy as np
import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "reproduce_f107_cadence.py"
spec = importlib.util.spec_from_file_location("reproduce_f107_cadence", SCRIPT)
study = importlib.util.module_from_spec(spec)
spec.loader.exec_module(study)


def series(start, end):
    return {start + timedelta(days=i): float(i + 101)
            for i in range((end - start).days + 1)}


def test_candidate_windows_cover_correct_calendar_days():
    daily = series(date(2024, 1, 1), date(2024, 3, 31))
    day = date(2024, 2, 15)
    got = study.candidate_flux(daily, day)
    assert got["operational_automatic"] is None
    assert got["trailing27_latency1"] == pytest.approx(np.mean(
        [daily[day - timedelta(days=i)] for i in range(1, 28)]))
    assert got["latest_daily_latency1"] == daily[date(2024, 2, 14)]
    assert got["previous_month_mean"] == pytest.approx(np.mean(
        [daily[date(2024, 1, i)] for i in range(1, 32)]))
    assert got["centered27_diagnostic"] == pytest.approx(np.mean(
        [daily[day + timedelta(days=i)] for i in range(-13, 14)]))
    assert got["same_month_finalized_diagnostic"] == pytest.approx(np.mean(
        [daily[date(2024, 2, i)] for i in range(1, 30)]))


def test_missing_window_day_is_an_error_not_a_partial_average():
    daily = series(date(2024, 1, 1), date(2024, 3, 31))
    del daily[date(2024, 2, 10)]
    with pytest.raises(ValueError, match="Incomplete solar interval"):
        study.candidate_flux(daily, date(2024, 2, 15))


def test_previous_month_window_handles_year_change():
    daily = series(date(2023, 11, 1), date(2024, 2, 28))
    got = study.candidate_flux(daily, date(2024, 1, 15))
    assert got["previous_month_mean"] == pytest.approx(np.mean(
        [daily[date(2023, 12, i)] for i in range(1, 32)]))


def test_paired_ci_is_reoriented_with_reversed_bounds():
    score = {"paired": {
        "operational_automatic_minus_trailing27_latency1": {
            "relative_mae_difference": 0.04,
            "relative_mae_ci95": [0.01, 0.08],
        },
    }}
    assert study.candidate_minus_automatic(score, "trailing27_latency1") == {
        "difference": -0.04, "ci95": [-0.08, -0.01],
    }


def test_reject_incorrect_checksum(tmp_path):
    file = tmp_path / "test.txt"
    file.write_text("not the right input")
    with pytest.raises(ValueError, match="expected sha256"):
        study.verify_sha256(file, "0" * 64)
