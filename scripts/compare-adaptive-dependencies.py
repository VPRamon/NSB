#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Check the dependency-only scientific export, independent of search changes."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / "docs/specifications/adaptive-acceptance"
a, b = ([json.loads(line) for line in (root / name).read_text().splitlines()]
        for name in ("dependency-A.jsonl", "dependency-B.jsonl"))
assert len(a) == len(b) == 67
assert a == b, "dependency upgrade changed the reference; investigate before accepting"
print("67 dependency-only reference cases identical (64 physical, 3 temporal)")
