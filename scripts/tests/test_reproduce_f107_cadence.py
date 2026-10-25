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


@pytest.mark.parametrize('text', [
    '20240101 100\n20240101 101\n',
    '20240101\n20240101 100\n',
    '20240101 nan\n', '20240101 inf\n', '20240101 -1\n',
    '20240101 100 extra\n', 'not-a-date 100\n', '20240230 100\n',
])
def test_malformed_solar_records_rejected(tmp_path, text):
    path = tmp_path / 'solar.txt'
    path.write_text(text)
    with pytest.raises(ValueError):
        study.solar_daily(path)


def test_missing_observations_are_preserved_as_missing(tmp_path):
    path = tmp_path / 'solar.txt'
    path.write_text('20240101\n20240102 .\n20240103 123.4\n')
    assert study.solar_daily(path) == {date(2024, 1, 3): 123.4}
    with pytest.raises(ValueError, match='missing 2024-01-01'):
        study.complete_mean(study.solar_daily(path), date(2024, 1, 1), date(2024, 1, 3))


@pytest.mark.parametrize('value', [np.nan, np.inf, 0, -10])
def test_window_rejects_invalid_values(value):
    day = date(2024, 1, 1)
    with pytest.raises(ValueError, match='Invalid observed'):
        study.complete_mean({day: value}, day, day)


def test_calendar_lag_excludes_same_day_but_previous_month_needs_last_day():
    daily = series(date(2023, 12, 1), date(2024, 3, 31))
    day = date(2024, 3, 1)
    before = study.candidate_flux(daily, day)
    assert before['previous_month_mean'] == pytest.approx(np.mean(
        [daily[date(2024, 2, i)] for i in range(1, 30)]))
    # Calendar cutoff only: actual release timestamps are not in this dataset.
    daily[day] = 999
    after = study.candidate_flux(daily, day)
    for name in ('trailing27_latency1', 'latest_daily_latency1', 'previous_month_mean'):
        assert before[name] == after[name]
    assert before['centered27_diagnostic'] != after['centered27_diagnostic']
    del daily[date(2024, 2, 29)]
    with pytest.raises(ValueError, match='missing 2024-02-29'):
        study.candidate_flux(daily, day)


def test_relative_paired_bootstrap_matches_materialized_nights():
    import ast
    from itertools import combinations
    import json
    nb = json.loads(study.NOTEBOOK.read_text())
    names = {'errors', 'nightly_errors', 'scores', 'condition_scores'}
    nodes = [node for cell in nb['cells'] if cell['cell_type'] == 'code'
             for node in ast.parse(''.join(cell['source'])).body
             if isinstance(node, ast.FunctionDef) and node.name in names]
    scope = {'np': np, 'combinations': combinations}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(study.NOTEBOOK), 'exec'), scope)
    y = np.array([1., 2., 3., 4., 5.])
    nights = np.array([1, 1, 2, 3, 4])
    predictions = {'operational_automatic': y + .4, 'candidate': y + [.1, .5, .2, .6, .1]}
    result = scope['scores'](y, predictions, nights, 2000, 182)
    paired = study.candidate_minus_automatic(result, 'candidate')
    unique = np.unique(nights)
    draws = np.random.default_rng(182).integers(4, size=(2000, 4))
    differences = []
    for draw in draws:
        idx = np.concatenate([np.flatnonzero(nights == unique[i]) for i in draw])
        differences.append(np.mean(abs(predictions['candidate'][idx] - y[idx]) / y[idx])
                           - np.mean(abs(predictions['operational_automatic'][idx] - y[idx]) / y[idx]))
    np.testing.assert_allclose(paired['ci95'], np.percentile(differences, [2.5, 97.5]))
    assert paired['difference'] == pytest.approx(np.mean(
        (abs(predictions['candidate'] - y) - abs(predictions['operational_automatic'] - y)) / y))
    assert result == scope['scores'](y, predictions, nights, 2000, 182)


def test_lesson_links_and_index():
    import re
    from urllib.parse import urlsplit, unquote
    root = study.ROOT
    index = root / 'docs/lessons-learned/README.md'
    assert 'lessons-learned/README.md' in (root / 'docs/README.md').read_text()
    for path in index.parent.glob('*.md'):
        if path != index:
            assert path.name in index.read_text()
        for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)', path.read_text()):
            uri = urlsplit(target)
            if not uri.scheme and uri.path:
                assert (path.parent / unquote(uri.path)).is_file(), (path, target)


def test_evidence_schema_and_numerical_consistency():
    import json
    artifact = json.loads(study.EVIDENCE.read_text())
    result = artifact['verified_reproduction']
    assert result['schema_version'] == 1
    assert result['reproduction_status'] == 'verified'
    assert (result['evaluation_spectra'], result['evaluation_nights']) == (94, 17)
    assert (result['bootstrap_draws'], result['seed']) == (2000, 182)
    assert set(result['primary_oxygen_metrics']) == set(study.NAMES)
    for name, digest in result['source_sha256'].items():
        assert (study.ROOT / name).is_file()
        assert len(digest) == 64 and all(c in "0123456789abcdef" for c in digest)
    base = result['within_training_domain_metrics']['operational_automatic']
    for name, paired in result['within_training_domain_paired'].items():
        metric = result['within_training_domain_metrics'][name]
        assert (paired['n'], paired['nights']) == (21, 8)
        assert paired['difference'] == pytest.approx(
            metric['mean_absolute_relative_error'] - base['mean_absolute_relative_error'])
        assert np.isfinite(paired['ci95']).all()
        assert paired['ci95'][0] <= paired['ci95'][1]
        full = result['full_cohort_paired'][name]
        assert full['difference'] == pytest.approx(
            result['primary_oxygen_metrics'][name]['relative_mae']
            - result['primary_oxygen_metrics']['operational_automatic']['relative_mae'])
    for name, field in [('patat', 'patat_sha256'), ('solar', 'solar_sha256'),
                        ('cohort', 'cohort_sha256'), ('rieke', 'rieke_sha256')]:
        assert result['input_sha256'][name] == artifact['inputs'][field]


def test_registered_partition_and_inventory_guard(tmp_path):
    import ast
    import json
    import tarfile
    nb = json.loads(study.NOTEBOOK.read_text())
    nodes = [node for cell in nb['cells'] if cell['cell_type'] == 'code'
             for node in ast.parse(''.join(cell['source'])).body
             if (isinstance(node, ast.FunctionDef) and node.name == 'load_cohort')
             or (isinstance(node, ast.Assign) and len(node.targets) == 1
                 and isinstance(node.targets[0], ast.Name)
                 and node.targets[0].id in {'COHORT_SHA', 'CALIBRATION_NIGHTS'})]
    scope = {'tarfile': tarfile}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(study.NOTEBOOK), 'exec'), scope)
    assert scope['CALIBRATION_NIGHTS'] == frozenset((51643, 51725, 51761, 51785, 52847))
    assert scope['COHORT_SHA'] == json.loads(study.EVIDENCE.read_text())['inputs']['cohort_sha256']
    path = tmp_path / 'wrong.tar'
    with tarfile.open(path, 'w'):
        pass
    with pytest.raises(ValueError, match='FITS inventory changed'):
        scope['load_cohort'](path)
