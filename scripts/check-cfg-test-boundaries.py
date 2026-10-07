#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Reject test-only implementation hooks in Rust production source trees."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

CFG_TEST = re.compile(r"^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*$")
MODULE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*(?:;|\{)")
ATTRIBUTE = re.compile(r"^\s*#\s*\[")


def rust_sources(root: Path) -> list[Path]:
    return sorted(path for path in root.glob("crates/*/src/**/*.rs") if path.is_file())


def next_item(lines: list[str], start: int) -> tuple[int, str] | None:
    index = start
    while index < len(lines):
        if not lines[index].strip():
            index += 1
            continue
        if ATTRIBUTE.match(lines[index]):
            depth = lines[index].count("[") - lines[index].count("]")
            index += 1
            while depth > 0 and index < len(lines):
                depth += lines[index].count("[") - lines[index].count("]")
                index += 1
            continue
        return index, lines[index]
    return None


def violations(path: Path) -> list[tuple[int, int, str]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    found: list[tuple[int, int, str]] = []
    for index, line in enumerate(lines):
        if not CFG_TEST.match(line):
            continue
        item = next_item(lines, index + 1)
        if item is None:
            found.append((index + 1, index + 1, "<end of file>"))
            continue
        item_index, item_line = item
        if not MODULE.match(item_line):
            found.append((index + 1, item_index + 1, item_line.strip()))
    return found


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Require #[cfg(test)] in crates/*/src to gate test modules only."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (default: inferred from this script)",
    )
    args = parser.parse_args()

    root = args.root.resolve()
    failures: list[tuple[Path, int, int, str]] = []
    for path in rust_sources(root):
        for cfg_line, item_line, item in violations(path):
            failures.append((path, cfg_line, item_line, item))

    if not failures:
        print("cfg(test) boundary check: PASS")
        return 0

    print(
        "cfg(test) boundary check: FAIL\n"
        "#[cfg(test)] may only gate dedicated test modules under production src/ trees.\n",
        file=sys.stderr,
    )
    for path, cfg_line, item_line, item in failures:
        relative = path.relative_to(root)
        print(
            f"{relative}:{cfg_line}: cfg(test) gates non-module item "
            f"at line {item_line}: {item}",
            file=sys.stderr,
        )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
