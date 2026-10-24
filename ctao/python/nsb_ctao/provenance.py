"""Execution provenance / run-manifest helpers for the CTAO notebook."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


def _safe_import_version(name: str) -> str | None:
    try:
        mod = __import__(name)
    except Exception:
        return None
    return getattr(mod, "__version__", None) or getattr(mod, "VERSION", None)


def _git_revision(path: Path) -> str | None:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "HEAD"],
            cwd=path,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        return out.strip()
    except Exception:
        return None


def collect_run_manifest(
    *,
    repo_root: Path,
    mode: str,
    seed: int = 20260324,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    nsb_info: dict[str, Any] = {"installed": False}
    try:
        import nsb

        nsb_info = {
            "installed": True,
            "version": getattr(nsb, "__version__", None),
            "MODEL_VERSION": getattr(nsb, "MODEL_VERSION", None),
            "SIDERUST_VERSION": getattr(nsb, "SIDERUST_VERSION", None),
            "SIDERUST_SOURCE": getattr(nsb, "SIDERUST_SOURCE", None),
            "file": getattr(nsb, "__file__", None),
        }
    except Exception as exc:
        nsb_info["error"] = str(exc)

    nsb2_info: dict[str, Any] = {"installed": False}
    try:
        import nsb2  # type: ignore

        nsb2_info = {
            "installed": True,
            "version": getattr(nsb2, "__version__", None),
            "file": getattr(nsb2, "__file__", None),
        }
    except Exception:
        # nsb2 may be a script package without top-level import
        try:
            import importlib.util

            spec = importlib.util.find_spec("nsb2")
            nsb2_info = {
                "installed": spec is not None,
                "origin": getattr(spec, "origin", None) if spec else None,
            }
        except Exception as exc:
            nsb2_info["error"] = str(exc)

    deps = {
        name: _safe_import_version(name)
        for name in (
            "numpy",
            "scipy",
            "astropy",
            "matplotlib",
            "pandas",
            "siderust",
            "tempoch",
            "qtty",
        )
    }

    manifest = {
        "schema": "nsb-ctao-run-manifest-v1",
        "executed_at_utc": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "mode": mode,
        "reproducibility_seed": seed,
        "python": {
            "version": sys.version,
            "executable": sys.executable,
            "platform": platform.platform(),
        },
        "validation_package": {
            "git_revision": _git_revision(repo_root),
            "repo_root_name": repo_root.name,
        },
        "nsb": nsb_info,
        "nsb2": nsb2_info,
        "dependencies": deps,
        "lvm_absolute_equivalence": os.environ.get(
            "LVM_ABSOLUTE_EQUIVALENCE", "PENDING_EXTERNAL_EVIDENCE"
        ),
        "oxygen_default_admission": "NO-GO (reassessment v2; LVM gate pending)",
        "extra": extra or {},
    }
    return manifest


def write_json(path: Path, payload: dict[str, Any]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    path.write_text(text, encoding="utf-8")
    return hashlib.sha256(text.encode()).hexdigest()
