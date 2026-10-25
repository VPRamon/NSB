# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Test the standalone notebook's actual inline helpers without scientific assets.

Run: .venv/bin/python -m pytest scripts/tests/test_observational_statistics.py -q
Only NumPy and pytest are required; no model imports, downloads or calibration.
"""
import ast
from itertools import combinations
import json
from pathlib import Path

import numpy as np
import pytest

NOTEBOOK = Path(__file__).resolve().parents[2] / 'examples/observational-validation.ipynb'
FUNCTIONS = {'errors', 'nightly_errors', 'condition_scores', 'scores',
             'radiance_interval', 'cluster_statistics_interpretation'}
namespace = {'np': np, 'combinations': combinations,
             'DISPLAY_NAMES': {'NSB continuum': 'NSB continuum',
                               'NSB Automatic': 'NSB Automatic (Oxygen)',
                               'nsb2': 'nsb2', 'baseline': 'Calibration baseline'},
             'SCENARIOS': {'native': 'Native solar inputs',
                           'matched': 'Matched measured F10.7'},
             'UNIT': 'photons cm^-2 ns^-1 sr^-1'}
notebook = json.loads(NOTEBOOK.read_text())
nodes = [node for cell in notebook['cells'] if cell['cell_type'] == 'code'
         for node in ast.parse(''.join(cell['source'])).body
         if isinstance(node, ast.FunctionDef) and node.name in FUNCTIONS]
assert {node.name for node in nodes} == FUNCTIONS
exec(compile(ast.Module(body=nodes, type_ignores=[]), str(NOTEBOOK), 'exec'), namespace)
errors, nightly_errors, condition_scores, scores = (namespace[n] for n in
                                                   ('errors', 'nightly_errors', 'condition_scores', 'scores'))
cluster_statistics_interpretation = namespace['cluster_statistics_interpretation']


def sample():
    # Unequal cluster sizes, mixed signs; expected pooled MAE=4, nightly MAE=14/3.
    return np.full(4, 10.), {'a': np.array([12., 12., 14., 2.]),
                            'b': np.array([11., 11., 11., 11.])}, np.array([1, 1, 2, 3])


def test_per_night_weighting_and_sign():
    y, p, nights = sample()
    result = scores(y, p, nights)
    per_night = result['per_night']
    assert [per_night[n]['a']['mae'] for n in (1, 2, 3)] == [2, 4, 8]
    assert [per_night[n]['a']['n'] for n in (1, 2, 3)] == [2, 1, 1]
    assert per_night[3]['a']['mean_signed_error'] == -8
    assert per_night[1]['a']['mean_absolute_relative_error'] == pytest.approx(.2)
    assert result['models']['a']['mae'] == 4
    assert result['models']['a']['night_weighted_mae'] == pytest.approx(14/3)
    assert result['models']['a']['mean_signed_error'] == 0
    pair = result['paired']['a_minus_b']
    assert pair['mae_difference'] == 3
    assert pair['bias_difference'] == -1


def test_bootstrap_matches_explicit_complete_cluster_resampling():
    y, p, nights = sample()
    result = scores(y, p, nights, 300, 182)
    # Independent oracle materializes spectra: duplicate a sampled night in full.
    rng = np.random.default_rng(182)
    sampled_nights = rng.integers(1, 4, size=(300, 3))
    draws = {k: [] for k in p}
    for draw in sampled_nights:
        indices = np.concatenate([np.flatnonzero(nights == n) for n in draw])
        for k in p:
            residual = p[k][indices] - y[indices]
            draws[k].append([np.mean(abs(residual)), np.mean(abs(residual)/y[indices]), np.mean(residual)])
    for k in p:
        draws[k] = np.array(draws[k])
        for column, metric in enumerate(('mae', 'relative_mae', 'bias')):
            np.testing.assert_allclose(result['models'][k][f'{metric}_ci95'],
                                       np.percentile(draws[k][:, column], [2.5, 97.5]))
    for column, metric in enumerate(('mae', 'relative_mae', 'bias')):
        np.testing.assert_allclose(result['paired']['a_minus_b'][f'{metric}_ci95'],
                                   np.percentile(draws['a'][:, column] - draws['b'][:, column],
                                                 [2.5, 97.5]))
    assert result == scores(y, p, nights, 300, 182)


@pytest.mark.parametrize('denominator', [0, -1, np.nan, np.inf])
def test_invalid_relative_denominators_rejected_without_dropping(denominator):
    y, p, nights = sample()
    y[0] = denominator
    with pytest.raises(ValueError):
        nightly_errors(y, p, nights)
    with pytest.raises(ValueError):
        scores(y, p, nights)
    with pytest.raises(ValueError):
        condition_scores(y, p, nights, np.ones(4, dtype=bool))


@pytest.mark.parametrize('mask, count, independent', [
    ([False]*4, 0, 0), ([True, True, False, False], 2, 1),
    ([True, True, True, False], 3, 2), ([True]*4, 4, 3)])
def test_sparse_and_empty_conditions(mask, count, independent):
    y, p, nights = sample()
    group = condition_scores(y, p, nights, np.array(mask))
    assert (group['n'], group['nights']) == (count, independent)
    if independent < 3:
        assert all('bias_ci95' not in m for m in group['models'].values())
        assert all('bias_ci95' not in m for m in group['paired'].values())
    else:
        assert 'sparse' in group['coverage']
        assert 'bias_ci95' in group['models']['a']
    if count:
        subset = errors(y[mask], p['a'][mask])
        assert group['models']['a']['mae'] == subset['mae']
        assert group['paired']['a_minus_b']['bias_difference'] == pytest.approx(
            subset['mean_signed_error'] - errors(y[mask], p['b'][mask])['mean_signed_error'])


def test_fewer_than_three_nights_rejected_for_unconditional_intervals():
    with pytest.raises(ValueError, match='three independent nights'):
        scores([10, 10], {'a': [12, 8]}, [1, 2])



def test_incomplete_campaign_has_no_cached_scientific_conclusion():
    y, p, nights = sample()
    native = scores(y, p, nights)
    message = cluster_statistics_interpretation({'native': native}, {}, [])
    assert 'Conditional interpretation unavailable' in message
    assert '0.05750' not in message
    # Even a complete pair must not get a conclusion if predictions failed.
    message = cluster_statistics_interpretation(
        {'native': native, 'matched': native}, {}, [{'scenario': 'matched'}])
    assert 'Conditional interpretation unavailable' in message


def test_scientific_interpretation_uses_current_metrics_not_fixed_snapshot():
    y = np.full(6, 10.0)
    nights = np.array([1, 1, 2, 3, 4, 4])
    predictions = {
        'NSB continuum': np.array([12., 12., 14., 10., 9., 8.]),
        'NSB Automatic': np.array([11., 12., 12., 10., 9., 8.]),
        'nsb2': np.array([10., 10., 9., 11., 11., 10.]),
        'baseline': np.full(6, 10.),
    }
    masks = {
        'Airmass [1, 1.25]': np.array([True, True, True, False, False, False]),
        'Airmass (1.25, 1.5]': np.array([False, False, False, True, True, True]),
        'F27 <67 sfu (outside training)': np.zeros(6, dtype=bool),
        'F27 67–166 sfu (within training)': np.array(
            [True, True, True, False, False, False]),
        'F27 >166 sfu (outside training)': np.array(
            [False, False, False, True, True, True]),
    }

    def report(oxygen_offset):
        current = {name: values.copy() for name, values in predictions.items()}
        current['NSB Automatic'] += oxygen_offset
        aggregates = {scenario: scores(y, current, nights, 200)
                      for scenario in ('native', 'matched')}
        conditions = {
            'matched': {name: condition_scores(y, current, nights, mask, 200)
                        for name, mask in masks.items()}
        }
        return cluster_statistics_interpretation(aggregates, conditions, []), aggregates

    first, metrics = report(0.)
    second, changed = report(2.)
    expected = f"{metrics['matched']['models']['NSB Automatic']['mae']:.5f}"
    updated = f"{changed['matched']['models']['NSB Automatic']['mae']:.5f}"
    assert f"MAE is {expected}" in first
    assert f"MAE is {updated}" in second
    assert first != second
    assert '0.05750' not in first  # No historical study estimate baked into the helper.
    assert 'F27 <67 sfu has 0 spectra / 0 nights' in first
    assert 'no blanket directional conclusion is justified' in first
