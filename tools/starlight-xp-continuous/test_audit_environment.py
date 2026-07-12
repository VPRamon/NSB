#!/usr/bin/env python3
"""Smoke tests for GaiaXPy environment audit."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

_TOOL_ROOT = Path(__file__).resolve().parent
_PINNED_VENV_PYTHON = _TOOL_ROOT / ".venv" / "bin" / "python"


def resolve_audit_python() -> str:
    """Prefer the pinned tool venv; fall back to the active interpreter."""
    if _PINNED_VENV_PYTHON.is_file():
        return str(_PINNED_VENV_PYTHON)
    return sys.executable


def gaiaxpy_importable(python: str) -> bool:
    probe = subprocess.run(
        [python, "-c", "import gaiaxpy"],
        capture_output=True,
        text=True,
        check=False,
    )
    return probe.returncode == 0


_AUDIT_PYTHON = resolve_audit_python()
_GAIAXPY_AVAILABLE = gaiaxpy_importable(_AUDIT_PYTHON)


@unittest.skipUnless(
    _GAIAXPY_AVAILABLE,
    "GaiaXPy not installed; create tools/starlight-xp-continuous/.venv "
    "with requirements.txt (GaiaXPy==2.1.4)",
)
class AuditEnvironmentTests(unittest.TestCase):
    def test_audit_writes_checksum(self) -> None:
        script = Path(__file__).with_name("audit_gaiaxpy_environment.py")
        with tempfile.TemporaryDirectory(prefix="phase5_audit_") as tmp:
            tmp_path = Path(tmp)
            out_json = tmp_path / "phase5_gaiaxpy_environment.json"
            out_sha = tmp_path / "phase5_gaiaxpy_environment.sha256"
            subprocess.run(
                [
                    _AUDIT_PYTHON,
                    str(script),
                    "--output-json",
                    str(out_json),
                    "--output-sha256",
                    str(out_sha),
                ],
                check=True,
            )
            payload = json.loads(out_json.read_text(encoding="utf-8"))
            self.assertEqual(payload["gaiaxpy_version"], "2.1.4")
            self.assertGreaterEqual(len(payload["calibration_data"]["files"]), 10)
            self.assertTrue(out_sha.read_text(encoding="utf-8").strip())


if __name__ == "__main__":
    unittest.main()
