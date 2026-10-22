# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Installed-wheel equivalence: NSB_INTERFACE_TESTS=1; build nsb-cli first."""
import json
import math
import os
from pathlib import Path
import subprocess

import nsb
import pytest

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "crates/nsb/tests/data/interfaces"
# Same kernels, with only serialization/ABI rounding permitted. Absolute floor
# covers physically zero components; this is not a model uncertainty tolerance.
RTOL, ATOL = 2e-12, 1e-14
pytestmark = pytest.mark.skipif(os.environ.get("NSB_INTERFACE_TESTS") != "1",
                              reason="enable installed Rust/Python/CLI checks explicitly")


def python_result(document, case):
    import nsb
    import siderust
    from datetime import datetime
    config = (nsb.NsbModelConfig.generic_clear_sky()
              .with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_V1)
              .with_solar_radio_flux_sfu(case["sfu"]))
    evaluator = nsb.NsbEvaluator(config)
    mask = nsb.ComponentMask.ZODIACAL & nsb.ComponentMask.AIRGLOW
    for name in case["components"]:
        mask = mask | getattr(nsb.ComponentMask, name.upper())
    observer, target = document["observer"], document["target"]
    query = nsb.PointQuery(
        siderust.Observer(observer["lon"], observer["lat"], observer["height"]),
        datetime.fromisoformat(document["time"].replace("Z", "+00:00")),
        siderust.Direction(target["ra"], target["dec"]), components=mask)
    result = evaluator.evaluate(query)
    return {"id": case["id"], "total": result.integrated_photons_cm2_ns_sr,
            "components": {c.name: c.integrated_photons_cm2_ns_sr for c in result.components},
            "bands": [evaluator.integrate_band(query, a, b) for a, b in
                      [(300, 650), (300, 450.25), (450.25, 650), (360, 570)]]}


def cli_arguments(binary, document, case):
    observer, target = document["observer"], document["target"]
    return [binary, "--format", "json", "point", "--time", document["time"],
            "--lon", str(observer["lon"]), "--lat", str(observer["lat"]),
            "--height", str(observer["height"]), "--ra", str(target["ra"]),
            "--dec", str(target["dec"]), "--components", ",".join(case["components"]),
            "--site-profile", "generic-clear-sky", "--airglow-model", "paranal-palace-v1",
            "--solar-radio-flux-sfu", str(case["sfu"]), "--moonlight-model", "jones2013",
            "--zodiacal-extinction", "noll2012"]




def close(a, b):
    assert math.isfinite(a) and math.isfinite(b)
    assert abs(a - b) <= ATOL + RTOL * max(abs(a), abs(b)), (a, b)


@pytest.fixture(scope="module")
def interfaces(tmp_path_factory):
    outside = tmp_path_factory.mktemp("nsb-interfaces")
    output = outside / "rust.json"
    env = dict(os.environ, NSB_INTERFACE_OUTPUT=str(output))
    subprocess.run(["cargo", "test", "--locked", "-p", "nsb", "--test",
                    "interface_equivalence"], cwd=ROOT, env=env, check=True)
    document = json.loads((FIXTURES / "inputs.json").read_text())
    rust = json.loads(output.read_text())
    expected = json.loads((FIXTURES / "expected.json").read_text())
    assert len(document["cases"]) == len(rust) == len(expected)
    binary = Path(os.environ.get("NSB_CLI", ROOT / "target/debug/nsb")).resolve()
    assert binary.is_file(), "build cargo build --locked -p nsb-cli --bin nsb first"
    return outside, document, rust, expected, binary


@pytest.mark.parametrize("index", range(9))
def test_common_interfaces(interfaces, index):
    outside, document, rust, expected, binary = interfaces
    case, native, reference = document["cases"][index], rust[index], expected[index]
    python = python_result(document, case)
    assert python == python_result(document, case), "Python nondeterminism"
    args = cli_arguments(binary, document, case)
    raw = subprocess.check_output(args, cwd=outside, text=True)
    assert raw == subprocess.check_output(args, cwd=outside, text=True), "CLI nondeterminism"
    raw = json.loads(raw)
    cli = {"total": raw["total"]["integrated_ph_cm2_ns_sr"],
           "components": {c["name"]: c["integrated_ph_cm2_ns_sr"] for c in raw["components"]}}
    assert native["id"] == python["id"] == reference["id"] == case["id"]
    assert native["components"].keys() == python["components"].keys() == cli["components"].keys() == reference["components"].keys()
    for a, b in [(native, python), (native, cli), (python, cli), (native, reference)]:
        close(a["total"], b["total"])
        for component in a["components"]:
            close(a["components"][component], b["components"][component])
    for result in [native, python]:
        assert len(result["bands"]) == len(reference["bands"]) == 4
        for a, b in zip(result["bands"], reference["bands"]):
            close(a, b)
        close(result["total"], sum(result["components"].values()))
        close(result["total"], result["bands"][0])
        close(result["bands"][0], sum(result["bands"][1:3]))
    for a, b in zip(native["bands"], python["bands"]):
        close(a, b)


def test_zero_solar_flux_rejected(interfaces):
    outside, document, _, _, binary = interfaces
    invalid = json.loads(json.dumps(document))
    invalid["cases"][0]["sfu"] = 0.0
    result = subprocess.run(cli_arguments(binary, invalid, invalid["cases"][0]),
                            cwd=outside, text=True, capture_output=True)
    assert result.returncode != 0, "CLI accepted zero solar flux"
    with pytest.raises((nsb.NsbError, ValueError)):
        python_result(invalid, invalid["cases"][0])


def test_solar_flux_sweep(interfaces):
    _, document, rust, _, _ = interfaces
    sweep = [r["total"] for c, r in zip(document["cases"], rust)
             if c["id"].startswith("airglow-")]
    assert len(sweep) == 6
    assert all(b > a > 0 for a, b in zip(sweep, sweep[1:]))
