#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Check current user-facing documentation without rewriting pinned scientific evidence."""

import ast
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
    "examples/README.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
)
CURRENT_NOTEBOOKS = (
    "examples/observational-validation.ipynb",
    "examples/ctao-sky-background.ipynb",
    "examples/airglow-solar-activity.ipynb",
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


def notebook_issues(path: Path, nb: dict) -> list[str]:
    """Keep distributed notebooks parseable, runnable in order and output-free."""
    relative = path.relative_to(ROOT)
    if not isinstance(nb, dict) or nb.get("nbformat") != 4 or not isinstance(nb.get("cells"), list):
        return [f"{relative}: expected nbformat 4 cells"]
    problems, code = [], []
    for i, cell in enumerate(nb["cells"]):
        if not isinstance(cell, dict) or cell.get("cell_type") not in ("code", "markdown", "raw"):
            problems.append(f"{relative}:cell {i}: invalid cell type")
            continue
        source = cell.get("source")
        if not isinstance(source, list) or not all(isinstance(line, str) for line in source):
            problems.append(f"{relative}:cell {i}: invalid source")
            continue
        if cell["cell_type"] != "code":
            continue
        code.append(cell)
        if cell.get("outputs") != [] or cell.get("execution_count", "missing") is not None:
            problems.append(f"{relative}:cell {i}: committed outputs or execution count")
        try:
            ast.parse("".join(source), feature_version=(3, 12))
        except SyntaxError as error:
            problems.append(f"{relative}:cell {i}: invalid Python: {error}")
    if not code or "setup" not in code[0].get("metadata", {}).get("tags", []):
        problems.append(f"{relative}: installation setup must be the first code cell")
    else:
        try:
            tree = ast.parse("".join(code[0]["source"]))
            imports = {name.name.split(".")[0] for node in ast.walk(tree) if isinstance(node, ast.Import) for name in node.names}
            imports.update(node.module.split(".")[0] for node in ast.walk(tree) if isinstance(node, ast.ImportFrom) and node.module)
            if imports - sys.stdlib_module_names:
                problems.append(f"{relative}: setup imports third-party dependencies before installation")
            requirements = next(ast.literal_eval(node.value) for node in tree.body
                                if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "REQUIREMENTS" for t in node.targets))
            if not isinstance(requirements, list) or "nsb-py==0.2.0" not in requirements:
                problems.append(f"{relative}: setup must select nsb-py==0.2.0")
        except (SyntaxError, ValueError, StopIteration):
            problems.append(f"{relative}: setup requires a literal REQUIREMENTS list")
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
        try:
            nb = json.loads(path.read_text(encoding="utf-8"))
        except ValueError as error:
            problems.append(f"{name}: invalid notebook JSON: {error}")
            continue
        problems.extend(notebook_issues(path, nb))
        if not isinstance(nb, dict) or not isinstance(nb.get("cells"), list):
            continue
        for i, cell in enumerate(nb["cells"]):
            if isinstance(cell, dict) and cell.get("cell_type") == "markdown" and isinstance(cell.get("source"), list):
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
