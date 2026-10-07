#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Validate SPDX license headers in tracked source and CI workflow files."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


LICENSE = "AGPL-3.0-only"
COPYRIGHT = "Copyright (C) 2026 Vallés Puig, Ramon"
COMMENT_PREFIXES = {
    ".py": "#",
    ".pyi": "#",
    ".rs": "//",
    ".sh": "#",
    ".yaml": "#",
    ".yml": "#",
}


def tracked_files() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], check=True, capture_output=True
    )
    return [Path(path.decode()) for path in result.stdout.split(b"\0") if path]


def expected_header(prefix: str) -> tuple[str, str]:
    return (
        f"{prefix} SPDX-License-Identifier: {LICENSE}",
        f"{prefix} {COPYRIGHT}",
    )


def has_expected_header(path: Path) -> bool:
    prefix = COMMENT_PREFIXES.get(path.suffix)
    if prefix is None:
        return True

    lines = path.read_text(encoding="utf-8").splitlines()
    if lines and lines[0].startswith("#!"):
        lines = lines[1:]
    return tuple(lines[:2]) == expected_header(prefix)


def main() -> int:
    missing = [path for path in tracked_files() if not has_expected_header(path)]
    if not missing:
        return 0

    print("Missing or invalid SPDX license header:", file=sys.stderr)
    for path in missing:
        print(f"  {path}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
