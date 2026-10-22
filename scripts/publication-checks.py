#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Execute publication checks, preserving each failure and command receipt."""
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import sysconfig
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target/publication/checks"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["PYO3_PYTHON"] = sys.executable
    # Managed CPython can keep libpython outside the system loader search path.
    if sys.platform.startswith("linux"):
        env["LD_LIBRARY_PATH"] = os.pathsep.join(filter(None, [
            sysconfig.get_config_var("LIBDIR"), env.get("LD_LIBRARY_PATH")]))
    checks = [
        ("format", ["cargo", "fmt", "--all", "--", "--check"]),
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--", "-D", "warnings"]),
        ("workspace-tests", ["cargo", "test", "--workspace", "--all-features", "--locked"]),
        ("photon-si-reference", ["cargo", "test", "-p", "nsb", "--lib", "--locked", "photon_energy_to_s10_matches_si_definition", "--", "--nocapture"]),
        ("no-default-features", ["cargo", "check", "-p", "nsb", "--lib", "--no-default-features", "--locked"]),
        ("metadata", [sys.executable, "scripts/check-release-metadata.py"]),
        ("license-headers", ["bash", "scripts/check-license-headers.sh"]),
        ("cfg-boundaries", ["bash", "scripts/check-cfg-test-boundaries.sh"]),
        ("example", [sys.executable, "examples/publication/run.py", "--build"]),
        ("python-tests", [sys.executable, "-m", "pytest", "python/tests", "-q"]),
        ("cargo-package", ["cargo", "package", "-p", "nsb", "--locked"]),
        ("publish-dry-run", ["cargo", "publish", "--manifest-path", "crates/nsb/Cargo.toml", "--dry-run", "--locked"]),
    ]
    identity = {"source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "working_tree": subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True),
                "python": sys.version, "platform": platform.platform(), "PYO3_PYTHON": sys.executable,
                "LD_LIBRARY_PATH": env.get("LD_LIBRARY_PATH")}
    receipts = []
    for name, args in checks:
        started = datetime.now(timezone.utc).isoformat()
        print(f"Running {name}", flush=True)
        with (OUT / f"{name}.log").open("w") as log:
            process = subprocess.run(args, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        receipts.append({"name": name, "command": args, "started_utc": started,
                         "finished_utc": datetime.now(timezone.utc).isoformat(),
                         "exit_code": process.returncode, "result": "PASS" if process.returncode == 0 else "FAIL",
                         "log": f"{name}.log"})
        if name == "photon-si-reference" and process.returncode == 0:
            for line in (OUT / f"{name}.log").read_text().splitlines():
                if line.startswith("NSB_PHOTON_SI_RECEIPT="):
                    value = json.loads(line.split("=", 1)[1])
                    (OUT / "photon-si-reference.json").write_text(json.dumps(value, indent=2) + "\n")
        (OUT / "verification-results.json").write_text(json.dumps({"environment": identity, "checks": receipts}, indent=2) + "\n")
    # Packaging failures remain failures even when expected under issue #265.
    return int(any(r["exit_code"] for r in receipts))


if __name__ == "__main__":
    raise SystemExit(main())
