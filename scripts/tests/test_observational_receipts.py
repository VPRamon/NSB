# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Exercise the standalone export code, without installing models or downloading data."""

import ast
import copy
import csv
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest
from test_observational_statistics import NOTEBOOK
from test_observational_statistics import namespace as statistics

FUNCTIONS = {
    "align",
    "baseline",
    "receipt_json",
    "receipt_provenance",
    "export_receipts",
}
notebook = json.loads(NOTEBOOK.read_text())
nodes = [
    node
    for cell in notebook["cells"]
    if cell["cell_type"] == "code"
    for node in ast.parse("".join(cell["source"])).body
    if isinstance(node, ast.FunctionDef) and node.name in FUNCTIONS
]
assert {node.name for node in nodes} == FUNCTIONS
namespace = {
    **statistics,
    "Path": Path,
    "json": json,
    "hashlib": hashlib,
    "MODELS": ("NSB continuum", "NSB Automatic", "nsb2"),
    "SEED": 182,
    "REPETITIONS": 2000,
    "conditions": {"all": None},
}
# Execute only named helper definitions from the repository-owned notebook, as in the statistics tests.
exec(compile(ast.Module(body=nodes, type_ignores=[]), str(NOTEBOOK), "exec"), namespace)  # noqa: S102
export_receipts = namespace["export_receipts"]


@pytest.fixture
def campaign():
    rows = [
        {
            "id": f"spectrum-{i}",
            "night": night,
            "split": split,
            "observed": (8.0, 12.0)[i] if i < 2 else 10.0,
            "airmass": 1.1 if i % 2 else 1.4,
            "f27": 100.0 if i % 2 else 180.0,
            "header_sfu": 110.0,
        }
        for i, (night, split) in enumerate(
            [
                (0, "calibration"),
                (0, "calibration"),
                (1, "evaluation"),
                (1, "evaluation"),
                (2, "evaluation"),
                (3, "evaluation"),
            ]
        )
    ]
    models = namespace["MODELS"]
    predictions, metrics, conditional = {}, {}, {}
    for scenario, offset in [("native", 0), ("matched", 1)]:
        vectors = {
            model: np.array([10.0, 10.0, 12.0, 12.0, 14.0, 2.0]) + j + offset
            for j, model in enumerate(models)
        }
        predictions[scenario] = {
            model: [{"id": r["id"], "total": p} for r, p in zip(rows, values)]
            for model, values in vectors.items()
        }
        vectors["baseline"] = np.full(6, 10.0)
        evaluation = {model: values[2:] for model, values in vectors.items()}
        metrics[scenario] = namespace["scores"](
            np.full(4, 10.0), evaluation, [1, 1, 2, 3]
        )
        conditional[scenario] = {
            "all": namespace["condition_scores"](
                np.full(4, 10.0),
                {k: v for k, v in evaluation.items() if k != "baseline"},
                [1, 1, 2, 3],
                np.ones(4, dtype=bool),
            )
        }
    return {
        "rows": rows,
        "predictions": predictions,
        "metrics": metrics,
        "conditional_metrics": conditional,
        "availability": {"native": "available", "matched": "available"},
        "failures": [],
        "provenance": {
            "revisions": {"nsb-py": None},
            "unavailable": {"nsb-py_revision": "not exposed"},
        },
    }


def read_receipts(path):
    with (path / "predictions.csv").open(newline="") as stream:
        records = list(csv.DictReader(stream))
    return (
        json.loads((path / "manifest.json").read_bytes()),
        json.loads((path / "metrics.json").read_bytes()),
        records,
    )


def test_schema_determinism_and_roundtrip(tmp_path, campaign):
    export_receipts(tmp_path, **campaign)
    before = {p.name: p.read_bytes() for p in tmp_path.iterdir()}
    # Prediction-list/dictionary order is not scientific identity; align by IDs.
    shuffled = copy.deepcopy(campaign)
    for model_predictions in shuffled["predictions"].values():
        for values in model_predictions.values():
            values.reverse()
    shuffled["metrics"] = dict(reversed(list(shuffled["metrics"].items())))
    export_receipts(tmp_path, **shuffled)
    assert before == {p.name: p.read_bytes() for p in tmp_path.iterdir()}
    manifest, metrics, records = read_receipts(tmp_path)
    assert manifest["schema_version"] == metrics["schema_version"] == 1
    assert metrics["aggregate"] == json.loads(
        namespace["receipt_json"](campaign["metrics"])
    )
    assert metrics["conditional"] == json.loads(
        namespace["receipt_json"](campaign["conditional_metrics"])
    )
    assert manifest["status"] == "complete"
    assert manifest["prediction_records"] == len(records) == 48
    assert manifest["partitions"] == {
        "calibration": {"spectra": 2, "nights": 1},
        "evaluation": {"spectra": 4, "nights": 3},
    }
    assert list(records[0]) == manifest["prediction_columns"]
    assert manifest["observation_order"] == [r["id"] for r in campaign["rows"]]
    for name, identity in manifest["files"].items():
        assert hashlib.sha256(before[name]).hexdigest() == identity["sha256"]
    assert manifest["provenance"]["revisions"]["nsb-py"] is None
    assert all(b"\r" not in b and b.endswith(b"\n") for b in before.values())
    assert metrics["aggregate"]["native"]["per_night"]["1"]["NSB continuum"]["n"] == 2
    assert {r["scenario"] for r in records} == {"native", "matched"}
    assert {r["model"] for r in records} == set(manifest["model_order"])
    assert all(
        r["unit"] == namespace["UNIT"]
        and r["wavelength_lower_nm"] == "365.0"
        and r["wavelength_upper_nm"] == "650.0"
        for r in records
    )


def test_independent_metrics_and_paired_cluster_intervals(tmp_path, campaign):
    export_receipts(tmp_path, **campaign)
    manifest, metrics, records = read_receipts(tmp_path)
    for scenario in manifest["scenario_order"]:
        draws, points = {}, {}
        for model in manifest["model_order"]:
            selected = [
                r
                for r in records
                if r["scenario"] == scenario
                and r["model"] == model
                and r["subset"] == "evaluation"
            ]
            observed = np.array([float(r["observed"]) for r in selected])
            predicted = np.array([float(r["predicted"]) for r in selected])
            nights = np.array([int(r["night_id"]) for r in selected])
            delta = predicted - observed
            np.testing.assert_array_equal(
                delta, [float(r["residual"]) for r in selected]
            )
            np.testing.assert_array_equal(
                abs(delta), [float(r["absolute_residual"]) for r in selected]
            )
            np.testing.assert_array_equal(
                delta / observed, [float(r["relative_residual"]) for r in selected]
            )
            points[model] = np.array(
                [abs(delta).mean(), (abs(delta) / observed).mean(), delta.mean()]
            )
            m = metrics["aggregate"][scenario]["models"][model]
            np.testing.assert_allclose(
                points[model],
                [m["mae"], m["mean_absolute_relative_error"], m["mean_signed_error"]],
            )
            assert m["night_weighted_mae"] == pytest.approx(
                np.mean([np.mean(abs(delta[nights == n])) for n in np.unique(nights)])
            )
            # Independent materialization of complete night clusters, including duplicates.
            bootstrap = np.random.default_rng(182).integers(0, 3, size=(2000, 3))
            values = []
            for sampled in bootstrap:
                indices = np.concatenate(
                    [np.flatnonzero(nights == np.unique(nights)[j]) for j in sampled]
                )
                d = delta[indices]
                values.append(
                    [abs(d).mean(), (abs(d) / observed[indices]).mean(), d.mean()]
                )
            draws[model] = np.array(values)
            for column, key in enumerate(("mae", "relative_mae", "bias")):
                np.testing.assert_allclose(
                    np.percentile(draws[model][:, column], [2.5, 97.5]),
                    m[f"{key}_ci95"],
                )
        for pair, result in metrics["aggregate"][scenario]["paired"].items():
            left, right = pair.split("_minus_")
            for column, key in enumerate(("mae", "relative_mae", "bias")):
                assert result[f"{key}_difference"] == pytest.approx(
                    (points[left] - points[right])[column]
                )
                np.testing.assert_allclose(
                    np.percentile((draws[left] - draws[right])[:, column], [2.5, 97.5]),
                    result[f"{key}_ci95"],
                )
    assert metrics["aggregate"]["native"]["models"]["NSB continuum"]["mae"] == 4
    assert (
        metrics["aggregate"]["native"]["models"]["NSB continuum"]["mean_signed_error"]
        == 0
    )
    assert metrics["aggregate"]["native"]["models"]["NSB continuum"][
        "night_weighted_mae"
    ] == pytest.approx(14 / 3)
    assert all(float(r["predicted"]) == 10 for r in records if r["model"] == "baseline")


@pytest.mark.parametrize(
    "problem",
    [
        "failure",
        "unavailable",
        "missing_scenario",
        "missing_model",
        "missing_id",
        "duplicate_id",
        "extra_id",
        "negative",
        "nan",
        "zero_observation",
        "invalid_split",
        "shared_night",
        "missing_f27",
        "nonfinite_provenance",
        "stale_metric_count",
        "wrong_seed",
        "missing_conditions",
        "infinite",
        "negative_observation",
        "duplicate_observation",
        "nonfinite_metric",
    ],
)
def test_invalid_campaign_never_publishes_manifest(tmp_path, campaign, problem):
    export_receipts(
        tmp_path, **campaign
    )  # Also ensure a failed rerun invalidates the old manifest.
    records = campaign["predictions"]["native"]["nsb2"]
    if problem == "failure":
        campaign["failures"].append(
            {"reason": "/private/local/path must not be serialized"}
        )
    elif problem == "unavailable":
        campaign["availability"]["matched"] = "UNAVAILABLE: /private/path"
    elif problem == "missing_scenario":
        del campaign["metrics"]["matched"]
    elif problem == "missing_model":
        del campaign["predictions"]["native"]["nsb2"]
    elif problem == "missing_id":
        records.pop()
    elif problem == "duplicate_id":
        records.append(records[0])
    elif problem == "extra_id":
        records.append({"id": "extra", "total": 1})
    elif problem in ("negative", "nan", "infinite"):
        records[0]["total"] = {"negative": -1, "nan": np.nan, "infinite": np.inf}[
            problem
        ]
    elif problem in ("zero_observation", "negative_observation"):
        campaign["rows"][0]["observed"] = 0 if problem == "zero_observation" else -1
    elif problem == "duplicate_observation":
        campaign["rows"][1]["id"] = campaign["rows"][0]["id"]
    elif problem == "missing_conditions":
        campaign["conditional_metrics"]["matched"].clear()
    elif problem == "invalid_split":
        campaign["rows"][0]["split"] = "holdout"
    elif problem == "shared_night":
        campaign["rows"][0]["night"] = 1
    elif problem == "missing_f27":
        del campaign["rows"][0]["f27"]
    elif problem == "nonfinite_metric":
        campaign["metrics"]["native"]["models"]["nsb2"]["mae"] = np.nan
    elif problem == "nonfinite_provenance":
        campaign["provenance"]["value"] = np.inf
    elif problem == "stale_metric_count":
        campaign["metrics"]["native"]["models"]["nsb2"]["n"] = 3
    elif problem == "wrong_seed":
        campaign["metrics"]["native"]["seed"] = 42
    with pytest.raises(ValueError):
        export_receipts(tmp_path, **campaign)
    assert not (tmp_path / "manifest.json").exists()
    assert b"/private/" not in b"".join(p.read_bytes() for p in tmp_path.iterdir())


def test_float_roundtrip_and_explicit_missing_json_values(tmp_path, campaign):
    value = np.nextafter(10.0, 11.0)
    campaign["predictions"]["native"]["nsb2"][0]["total"] = value
    export_receipts(tmp_path, **campaign)
    _, _, records = read_receipts(tmp_path)
    row = next(r for r in records if r["scenario"] == "native" and r["model"] == "nsb2")
    assert float(row["predicted"]) == value
    assert float(row["residual"]) == value - 8
    assert (
        namespace["receipt_json"]({"value": None, np.int64(2): np.float64(-0.0)})
        == b'{\n  "2": -0.0,\n  "value": null\n}\n'
    )


def test_provenance_explicit_missing_fields_and_no_local_install_details(monkeypatch):
    ns = namespace
    values = {
        "versions": {"nsb-py": "0.2.0", "nsb2": "0.2.0"},
        "LABEL": "development",
        "sys": SimpleNamespace(version="3.12.13 (private machine details)"),
        "PATAT_URLS": ("https://example.org/patat",),
        "PATAT_SHA": "a" * 64,
        "SOLAR_URL": "https://example.org/solar",
        "SOLAR_SHA": "b" * 64,
        "models": SimpleNamespace(reference_assets=[], fraction=0.8),
        "selection": {"identity_sha256": "c" * 64},
        "CALIBRATION_NIGHTS": {0},
        "SITE": {},
        "COMPONENTS": ("airglow",),
        "PRACTICAL_MARGIN": 0.1,
        "nsb": SimpleNamespace(MODEL_VERSION="nsb-model-v1"),
        "metadata": SimpleNamespace(
            version=lambda _: "test-version",
            distribution=lambda _: SimpleNamespace(
                read_text=lambda _: json.dumps(
                    {
                        "url": "file:///private/checkout",
                        "vcs_info": {"commit_id": "d" * 40},
                    }
                )
            ),
        ),
    }
    for key, value in values.items():
        monkeypatch.setitem(ns, key, value)
    result = ns["receipt_provenance"]()
    assert result["revisions"] == {"nsb-py": None, "nsb2": "d" * 40}
    assert result["notebook_revision"] is None
    assert set(result) >= {
        "software",
        "revisions",
        "unavailable",
        "assets",
        "cohort",
        "configuration",
        "solar",
        "bootstrap",
        "metric_definitions",
        "limitations",
    }
    assert "nsb_bundled_asset_checksums" in result["unavailable"]
    assert result["bootstrap"]["seed"] == 182
    assert result["bootstrap"]["repetitions"] == 2000
    assert "/private/" not in json.dumps(result)
    assert "private machine" not in json.dumps(result)
