#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Compute and verify the publication example with three installed interfaces.

--build builds the checkout's locked Rust artifacts and wheel. Without it,
existing artifacts are used (offline). References are never updated implicitly.
"""
import argparse
import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
UNIT = "photons cm^-2 ns^-1 sr^-1"
RTOL, ATOL = 2e-12, 1e-14


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(args, cwd=ROOT):
    return subprocess.check_output([str(a) for a in args], cwd=cwd, text=True)


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


def compare(rows, operation, a, b, pair, case):
    if not (math.isfinite(a) and math.isfinite(b)):
        raise AssertionError(f"nonfinite {operation}/{case}: {a}, {b}")
    error = abs(a - b)
    passed = error <= ATOL + RTOL * max(abs(a), abs(b))
    rows.append({"case": case, "operation": operation, "pair": pair, "a": a, "b": b,
                 "units": UNIT, "absolute_error": error,
                 "relative_error": error / max(abs(a), abs(b)) if max(abs(a), abs(b)) else 0,
                 "absolute_tolerance": ATOL, "relative_tolerance": RTOL,
                 "result": "PASS" if passed else "FAIL"})


def plot(results, document, output):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    sweep = [(c["sfu"], r["total"]) for c, r in zip(document["cases"], results)
             if c["id"].startswith("airglow-")]
    plt.rcParams.update({"font.size": 10, "axes.spines.top": False,
                        "axes.spines.right": False, "pdf.fonttype": 42})
    fig, ax = plt.subplots(figsize=(6.4, 4.2), layout="constrained")
    ax.plot(*zip(*sweep), "o-", color="#205a87", label="PALACE continuum, 300–650 nm")
    ax.set(xlabel="Solar radio flux F10.7 (sfu)",
           ylabel=r"Photon radiance (photons cm$^{-2}$ ns$^{-1}$ sr$^{-1}$)",
           title="Continuum airglow at fixed Paranal geometry")
    ax.grid(alpha=0.2)
    ax.legend(frameon=False, fontsize=9)
    fig.text(0.53, 0.18, "Planning model; no oxygen lines\nNo observational uncertainty inferred",
             fontsize=8, color="#444444")
    output.mkdir(parents=True, exist_ok=True)
    fig.savefig(output / "airglow-sweep.pdf", metadata={"CreationDate": None})
    fig.savefig(output / "airglow-sweep.png", dpi=220)
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "target/publication/results")
    parser.add_argument("--reference", type=Path, default=HERE / "expected/results.json")
    parser.add_argument("--register", action="store_true", help="Explicit maintainer-only reference registration")
    options = parser.parse_args()
    options.output = options.output.resolve()
    options.output.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["PYO3_PYTHON"] = sys.executable
    if options.build:
        subprocess.run(["cargo", "build", "--locked", "-p", "nsb-cli", "-p", "nsb",
                        "--example", "publication_probe"], cwd=ROOT, env=env, check=True)
        subprocess.run([sys.executable, "-m", "maturin", "build", "--release", "--locked", "--out",
                        str(ROOT / "target/publication/wheels")], cwd=ROOT, env=env, check=True)
        wheels = list((ROOT / "target/publication/wheels").glob("nsb_py-*.whl"))
        if len(wheels) != 1:
            raise RuntimeError(f"Expected exactly one wheel, found {wheels}")
        subprocess.run([sys.executable, "-m", "pip", "install", "--no-deps", "--force-reinstall", str(wheels[0])], check=True)
    binary = ROOT / "target/debug/nsb"
    probe = ROOT / "target/debug/examples/publication_probe"
    fixture = HERE / "config/fixtures.json"
    document = json.loads(fixture.read_text())
    numerical_process = subprocess.run([probe, "numerical"], text=True, capture_output=True)
    if numerical_process.stdout:
        (options.output / "numerical-verification.json").write_text(numerical_process.stdout)
    numerical_process.check_returncode()
    rust = json.loads(command([probe, fixture]))
    assert rust == json.loads(command([probe, fixture])), "Rust nondeterminism"
    rows, data = [], []
    with tempfile.TemporaryDirectory(prefix="nsb-installed-") as outside:
        for case, native in zip(document["cases"], rust, strict=True):
            python = python_result(document, case)
            assert python == python_result(document, case), "Python nondeterminism"
            args = cli_arguments(binary, document, case)
            raw = json.loads(command(args, cwd=outside))
            assert raw == json.loads(command(args, cwd=outside)), "CLI nondeterminism"
            cli = {"total": raw["total"]["integrated_ph_cm2_ns_sr"],
                   "components": {c["name"]: c["integrated_ph_cm2_ns_sr"] for c in raw["components"]}}
            (options.output / f"cli-{case['id']}.json").write_text(json.dumps(raw, indent=2) + "\n")
            assert native["components"].keys() == python["components"].keys() == cli["components"].keys()
            for pair, a, b in [("Rust/Python", native, python), ("Rust/CLI", native, cli), ("Python/CLI", python, cli)]:
                compare(rows, "point-total", a["total"], b["total"], pair, case["id"])
                for component in a["components"]:
                    compare(rows, f"point-{component}", a["components"][component], b["components"][component], pair, case["id"])
            for i, (a, b) in enumerate(zip(native["bands"], python["bands"], strict=True)):
                compare(rows, f"band-{i}", a, b, "Rust/Python", case["id"])
            for label, result in [("Rust", native), ("Python", python)]:
                compare(rows, "component-sum", result["total"], sum(result["components"].values()), label, case["id"])
                compare(rows, "full-band", result["total"], result["bands"][0], label, case["id"])
                compare(rows, "band-partition", result["bands"][0], sum(result["bands"][1:3]), label, case["id"])
            data.append(native)
        # Nonpositive explicit solar flux must fail in all three interfaces.
        invalid = json.loads(json.dumps(document))
        invalid["cases"][0]["sfu"] = 0.0
        invalid_path = options.output / "invalid-fixture.json"
        write(invalid_path, invalid)
        invalid_checks = {}
        for name, args in [("Rust", [probe, invalid_path]), ("CLI", cli_arguments(binary, invalid, invalid["cases"][0]))]:
            result = subprocess.run(args, cwd=outside, text=True, capture_output=True)
            invalid_checks[name] = {"exit_code": result.returncode, "stderr": result.stderr}
            assert result.returncode != 0, f"{name} accepted zero solar flux"
        import nsb
        import siderust
        try:
            python_result(invalid, invalid["cases"][0])
        except (nsb.NsbError, ValueError) as error:
            invalid_checks["Python"] = {"type": type(error).__name__, "message": str(error)}
        else:
            raise AssertionError("Python accepted zero solar flux")
    write(options.output / "results.json", data)
    if options.register:
        write(options.reference, data)
    else:
        expected = json.loads(options.reference.read_text())
        assert len(expected) == len(data)
        for actual, reference in zip(data, expected, strict=True):
            assert actual["id"] == reference["id"]
            compare(rows, "registered-total", actual["total"], reference["total"], "candidate/reference", actual["id"])
            assert actual["components"].keys() == reference["components"].keys()
            for name, value in actual["components"].items():
                compare(rows, f"registered-{name}", value, reference["components"][name], "candidate/reference", actual["id"])
            for i, value in enumerate(actual["bands"]):
                compare(rows, f"registered-band-{i}", value, reference["bands"][i], "candidate/reference", actual["id"])
    report = {"schema": "nsb-interface-equivalence-v1", "checks": rows,
              "zero_solar_flux": invalid_checks, "repeated_run_determinism": "PASS",
              "max_absolute_error": max(r["absolute_error"] for r in rows),
              "max_relative_error": max(r["relative_error"] for r in rows),
              "result": "PASS" if all(r["result"] == "PASS" for r in rows) else "FAIL"}
    write(options.output / "interface-equivalence.json", report)
    # Persist failure evidence before raising; never silently update expected data.
    if report["result"] != "PASS":
        raise AssertionError("equivalence/reference check failed; inspect interface-equivalence.json")
    sweep = [r["total"] for r in data if r["id"].startswith("airglow-")]
    assert all(b > a > 0 for a, b in zip(sweep, sweep[1:])), "F10.7 sweep must be positive and increasing"
    plot(data, document, options.output / "figures")
    provenance = {"source_commit": command(["git", "rev-parse", "HEAD"]).strip(),
                  "working_tree_status": command(["git", "status", "--porcelain"]),
                  "rustc": command(["rustc", "--version"]).strip(),
                  "cargo": command(["cargo", "--version"]).strip(),
                  "python": sys.version, "platform": platform.platform(),
                  "machine": platform.machine(),
                  "packages": {p: importlib.metadata.version(p) for p in ["nsb-py", "siderust", "tempoch", "maturin", "matplotlib", "pytest"]},
                  "lockfile_sha256": digest(ROOT / "Cargo.lock"), "fixture_sha256": digest(fixture),
                  "results_sha256": digest(options.output / "results.json"),
                  "binary_sha256": {"cli": digest(binary), "rust_probe": digest(probe)},
                  "siderust": "83d1ebb29ca1ae0b327d5784d13eda231d52c120",
                  "python_freeze": command([sys.executable, "-m", "pip", "freeze"]).splitlines()}
    write(options.output / "software-provenance.json", provenance)
    print(f"PASS: {len(rows)} comparisons, {len(data)} fixtures; max absolute {report['max_absolute_error']:.3g} {UNIT}")


if __name__ == "__main__":
    main()
