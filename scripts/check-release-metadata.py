#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Fail-closed preflight for the Rust/Python/CLI release metadata."""

from __future__ import annotations

import argparse
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"release metadata invalid: {message}")


def read_toml(relative: str) -> dict:
    return tomllib.loads((ROOT / relative).read_text(encoding="utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--tag", help="Exact tag to validate, e.g. v0.1.0 (required by publish.yml)"
    )
    args = parser.parse_args()

    core = read_toml("crates/nsb/Cargo.toml")["package"]
    cli = read_toml("crates/nsb-cli/Cargo.toml")
    data_tools = read_toml("crates/nsb-data-tools/Cargo.toml")
    python = read_toml("pyproject.toml")
    lockfile = read_toml("Cargo.lock")

    version = core["version"]
    require(re.fullmatch(r"\d+\.\d+\.\d+", version) is not None,
            f"expected a release version MAJOR.MINOR.PATCH, got {version!r}")
    require(core["name"] == "nsb", "the publishable Rust crate must be nsb")
    require(core.get("publish") == ["crates-io"],
            "crates/nsb must publish only to crates.io")
    require(core["license"] == "AGPL-3.0-only", "core license changed unexpectedly")
    require(core["rust-version"] == "1.89", "README minimum Rust version is 1.89")

    for name, manifest in [("nsb-cli", cli), ("nsb-data-tools", data_tools)]:
        package = manifest["package"]
        require(package["name"] == name, f"{name} package name changed")
        require(package["version"] == version,
                f"{name} version differs from nsb {version}")
        require(package.get("publish") is False,
                f"{name} is not intended for registry publication")
        dep = manifest["dependencies"]["nsb"]
        require(dep.get("version") == f"={version}" and dep.get("path") == "../nsb",
                f"{name} must depend on the exact local nsb release version")

    for name in ("nsb", "nsb-cli", "nsb-data-tools"):
        matches = [item for item in lockfile["package"] if item["name"] == name]
        require(len(matches) == 1 and matches[0]["version"] == version,
                f"Cargo.lock entry for {name} must match {version}")

    project = python["project"]
    maturin = python["tool"]["maturin"]
    require(project["name"] == "nsb-py", "PyPI project must be nsb-py")
    require("version" in project["dynamic"] and "version" not in project,
            "Python version must be derived from Cargo, not duplicated")
    require(project["requires-python"] == ">=3.10",
            "Python abi3 minimum must be Python 3.10")
    require(maturin["manifest-path"] == "crates/nsb/Cargo.toml"
            and maturin["module-name"] == "nsb"
            and maturin["bindings"] == "pyo3"
            and "python" in maturin["features"],
            "Maturin must build the nsb module from crates/nsb with PyO3")
    require(python["build-system"]["requires"] == ["maturin==1.15.0"],
            "PEP 517 builds must use the tested Maturin version")
    deps = set(project["dependencies"])
    require({"siderust>=0.5.1,<0.6", "tempoch>=0.5.0,<0.6"} <= deps,
            "Python must include the supported Siderust/Tempoch runtime dependencies")
    notices = maturin.get("include", [])
    require(any(isinstance(item, dict)
                and item.get("path") == "crates/nsb/THIRD_PARTY_NOTICES.md"
                and set(item.get("format", [])) == {"sdist", "wheel"}
                for item in notices),
            "source and wheel distributions must include scientific notices")

    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    require(re.search(r"(?m)^##\s+\[?" + re.escape(version) + r"\]?(?=\s|$)",
                      changelog) is not None,
            f"CHANGELOG.md needs a {version} release section")
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    for entry in ("crates.io/crates/nsb", "pypi.org/project/nsb-py/",
                  "cargo add nsb", "pip install nsb-py"):
        require(entry in readme, f"README missing distribution guidance: {entry}")

    require((ROOT / "crates/nsb/api/API_FROZEN").is_file()
            and (ROOT / "crates/nsb/api/public-api.txt").stat().st_size > 0,
            "reviewed public API freeze must be active")
    require("v[0-9]*" in (ROOT / ".github/workflows/publish.yml").read_text(),
            "tag-triggered release workflow is missing")

    workspace = read_toml("Cargo.toml")
    require(not workspace.get("patch") and not workspace.get("replace"),
            "release dependencies must not use workspace patches or replacements")
    registry = "registry+https://github.com/rust-lang/crates.io-index"
    for package in lockfile["package"]:
        if package["name"] in {"nsb", "nsb-cli", "nsb-data-tools"}:
            require("source" not in package, "workspace packages must be local")
        else:
            require(package.get("source") == registry,
                    f"{package['name']} must use the published crates.io registry")
            require(re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")) is not None,
                    f"{package['name']} needs a registry checksum")
    for name in ("siderust", "affn", "tempoch", "tempoch-core", "siderust-py",
                 "tempoch-py", "pyo3", "pyo3-ffi"):
        require(sum(p["name"] == name for p in lockfile["package"]) == 1,
                f"expected exactly one {name} provider identity")
    core_manifest = read_toml("crates/nsb/Cargo.toml")
    siderust = core_manifest["dependencies"]["siderust"]
    locked_siderust = next(p for p in lockfile["package"] if p["name"] == "siderust")
    require(siderust.get("version") == "=" + locked_siderust["version"]
            and set(siderust).isdisjoint({"git", "path", "branch", "rev", "tag"}),
            "Siderust must pin the registry release represented by public provenance")

    bridge = core_manifest["dependencies"]["siderust-py"]
    require(bridge.get("version") == "0.5.1"
            and set(bridge).isdisjoint({"git", "path", "branch", "rev", "tag"}),
            "siderust-py must require corrected registry releases >=0.5.1,<0.6")
    locked_bridge = next(p for p in lockfile["package"] if p["name"] == "siderust-py")
    require(re.fullmatch(r"0\.5\.[1-9]\d*", locked_bridge["version"]) is not None,
            "locked siderust-py must exclude the unsafe 0.5.0 release")

    if args.tag is not None:
        require(re.fullmatch(r"v\d+\.\d+\.\d+", args.tag) is not None,
                f"unexpected tag format {args.tag!r}")
        require(args.tag == f"v{version}",
                f"tag {args.tag!r} does not match package version v{version}")

    print(f"release metadata OK: nsb {version}, nsb-py {version}, "
          "workspace tools in sync; public API frozen")


if __name__ == "__main__":
    main()
