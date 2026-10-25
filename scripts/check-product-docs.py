#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Check current user-facing documentation without rewriting pinned scientific evidence."""

import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent

CURRENT_MARKDOWN = (
    "README.md",
    "crates/nsb/README.md",
    "crates/nsb-cli/README.md",
    "README_PYPI.md",
    "docs/README.md",
    "docs/python.md",
    "docs/scientific-validation.md",
    "docs/user-guide/getting-started.md",
    "docs/user-guide/README.md",
    "docs/user-guide/components.md",
    "docs/user-guide/observatory-customization.md",
    "docs/developer-guide/README.md",
    "docs/developer-guide/architecture.md",
    "docs/developer-guide/module-reference.md",
    "docs/developer-guide/public-api.md",
    "docs/developer-guide/testing.md",
    "docs/specifications/model-maturity.md",
    "docs/specifications/scientific-model.md",
    "docs/specifications/validation-consumer.md",
    "docs/nsb_components/zodiacal/README.md",
    "notebooks/README.md",
    "examples/README.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
)
CURRENT_NOTEBOOKS = (
    "notebooks/NSB_Validation.ipynb",
    "notebooks/CTAO_API_Example.ipynb",
    "examples/scientific-reproducibility.ipynb",
)
LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
OUTDATED = (
    ("nsb-validation", "historical validation-repository reference"),
    ("PR #", "development pull-request narrative"),
)


def markdown_issues(path: Path, text: str) -> list[str]:
    problems = []
    relative = path.relative_to(ROOT)
    for token, reason in OUTDATED:
        if token.lower() in text.lower():
            problems.append(f"{relative}: {reason}: {token}")

    for match in LINK.finditer(text):
        target = match.group(1).strip().split(' "', 1)[0]
        if not target or target.startswith(("#", "mailto:", "data:")):
            continue
        uri = urlsplit(target)
        if uri.scheme or uri.netloc:
            continue
        local = (path.parent / unquote(uri.path)).resolve()
        try:
            local.relative_to(ROOT)
        except ValueError:
            problems.append(f"{relative}: link escapes repository: {target}")
            continue
        if not local.exists():
            line = text.count("\n", 0, match.start()) + 1
            problems.append(f"{relative}:{line}: missing local link {target}")
    return problems


def check() -> list[str]:
    problems = []
    for name in CURRENT_MARKDOWN:
        path = ROOT / name
        if not path.is_file():
            problems.append(f"{name}: current guide is missing")
            continue
        problems.extend(markdown_issues(path, path.read_text(encoding="utf-8")))

    for name in CURRENT_NOTEBOOKS:
        path = ROOT / name
        if not path.is_file():
            problems.append(f"{name}: notebook is missing")
            continue
        nb = json.loads(path.read_text(encoding="utf-8"))
        for i, cell in enumerate(nb["cells"]):
            if cell.get("cell_type") == "markdown":
                text = "".join(cell.get("source", []))
                problems.extend(markdown_issues(path, text))

    version = tomllib.loads((ROOT / "crates/nsb/Cargo.toml").read_text())["package"]["version"]
    citation = (ROOT / "CITATION.cff").read_text(encoding="utf-8")
    if f'version: "{version}"' not in citation:
        problems.append(f"CITATION.cff: must cite current crate version {version}")
    return problems


if __name__ == "__main__":
    issues = check()
    if issues:
        print("\n".join(issues), file=sys.stderr)
        raise SystemExit(1)
    print("Public docs: current navigation, links and citation OK")
