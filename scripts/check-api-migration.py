#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Check frozen snapshot compatibility, allowing only PR #314's exact migration.

Additions require no allowlist updates. Every historical line must survive,
with the four reviewed TargetDirection substitutions as the only exceptions.
This supplements cargo-public-api's structural comparison during migration.
"""
import json
import sys
from pathlib import Path


def check(before: str, after: str, migrations: list[dict[str, str]]) -> bool:
    replacements = {item["before"]: item["after"] for item in migrations}
    current = set(after.splitlines())
    used_migration = False
    missing = []
    for line in before.splitlines():
        if not line or line in current:
            continue
        replacement = replacements.get(line)
        if replacement is not None and replacement in current:
            used_migration = True
        else:
            missing.append(line)
    if missing:
        raise ValueError("unapproved removed/changed public API:\n" + "\n".join(missing))
    return used_migration


if __name__ == "__main__":
    try:
        used = check(Path(sys.argv[1]).read_text(), Path(sys.argv[2]).read_text(),
                     json.loads(Path(sys.argv[3]).read_text()))
    except ValueError as error:
        sys.exit(str(error))
    print("migration" if used else "strict")
