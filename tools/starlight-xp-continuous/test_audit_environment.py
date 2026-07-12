#!/usr/bin/env python3
"""Smoke tests for GaiaXPy environment audit."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


def test_audit_writes_checksum(tmp_path: Path) -> None:
    script = Path(__file__).with_name("audit_gaiaxpy_environment.py")
    out_json = tmp_path / "phase5_gaiaxpy_environment.json"
    out_sha = tmp_path / "phase5_gaiaxpy_environment.sha256"
    subprocess.run(
        [sys.executable, str(script), "--output-json", str(out_json), "--output-sha256", str(out_sha)],
        check=True,
    )
    payload = json.loads(out_json.read_text(encoding="utf-8"))
    assert payload["gaiaxpy_version"] == "2.1.4"
    assert len(payload["calibration_data"]["files"]) >= 10
    assert out_sha.read_text(encoding="utf-8").strip()


if __name__ == "__main__":
    test_audit_writes_checksum(Path("/tmp/phase5_audit_test"))
    print("audit environment test passed")
