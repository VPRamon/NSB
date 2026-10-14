#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Check the NSB-owned public schema baseline for the first 0.1.0 release.

Scientific Starlight candidate/runtime IDs are deliberately out of scope:
their exact bytes, hashes, and approval evidence are immutable.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
FIRST_RELEASE = "0.1.0"
EXPECTED_CSV = {
    "nsb-cli-point-csv-v1",
    "nsb-cli-point-csv-uncertainty-v1",
    "nsb-cli-window-csv-v1",
}
EXPECTED_JSON = {
    "nsb-cli-point-json-v1",
    "nsb-cli-window-json-v1",
}
EXPECTED_MODEL = "nsb-model-v1"
CLI_SCHEMA_PATTERN = re.compile(
    r"nsb-cli-(?:point|window)-(?:csv|json)(?:-uncertainty)?-v[0-9]+"
)


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def public_schema_ids(source: str) -> set[str]:
    return set(CLI_SCHEMA_PATTERN.findall(source))


def main() -> int:
    cargo = read("crates/nsb/Cargo.toml")
    version_match = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.MULTILINE)
    if version_match is None:
        print("Cannot determine nsb crate version", file=sys.stderr)
        return 1
    if version_match.group(1) != FIRST_RELEASE:
        print("First-release schema gate skipped after 0.1.0")
        return 0

    errors = []
    sources = (
        ("crates/nsb-cli/src/output/csv.rs", EXPECTED_CSV),
        ("crates/nsb-cli/src/output/json.rs", EXPECTED_JSON),
    )
    for path, expected in sources:
        actual = public_schema_ids(read(path))
        if actual != expected:
            errors.append(
                f"{path}: expected {sorted(expected)}, found {sorted(actual)}"
            )

    docs = read("docs/specifications/cli-schemas.md")
    actual_docs = public_schema_ids(docs)
    all_expected = EXPECTED_CSV | EXPECTED_JSON
    if actual_docs != all_expected:
        errors.append(
            f"CLI schema documentation: expected {sorted(all_expected)}, "
            f"found {sorted(actual_docs)}"
        )

    lib = read("crates/nsb/src/lib.rs")
    model = re.search(
        r'^pub const MODEL_VERSION:\s*&str\s*=\s*"([^"]+)"\s*;',
        lib,
        re.MULTILINE,
    )
    if model is None or model.group(1) != EXPECTED_MODEL:
        errors.append(f"MODEL_VERSION must be {EXPECTED_MODEL!r} for 0.1.0")

    if errors:
        print("First-release public schema baseline drift:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("NSB 0.1.0 public schema and model identities: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
