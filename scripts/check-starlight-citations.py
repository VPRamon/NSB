#!/usr/bin/env python3
"""Check Starlight citations and required notices in shipping archives.

This is a documentation/packaging consistency check, not a legal rights grant.
It intentionally never changes the #103 human redistribution decision.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import tarfile
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parents[1]
STELLAR_NOTICE = (
    "XHIP",
    "Hipparcos-2",
    "Anderson",
    "Francis",
    "van Leeuwen",
    "Gaia",
    "SVO",
    "CDS",
)
REQUIRED = {
    "docs/nsb_components/starlight/licensing/SCIENTIFIC_REFERENCES.md": (
        "2012AstL...38..331A",
        "10.1051/0004-6361:20078357",
        "10.26093/cds/vizier",
        "2000A&A...355L..27H",
        "Hipparcos/Hipparcos.Hp_bes",
        "10.17909/t9-khb7-4049",
        "10.1051/0004-6361/202243680",
        "10.1051/0004-6361/202243880",
        "2026-10-08",
        "pending",
    ),
    "docs/nsb_components/starlight/licensing/ATTRIBUTION.md": (
        "SCIENTIFIC_REFERENCES.md",
        "2012AstL...38..331A",
        "10.1051/0004-6361:20078357",
        "10.26093/cds/vizier",
        "This research has made use of the SVO Filter Profile Service",
    ),
    "THIRD_PARTY_NOTICES.md": (
        "SCIENTIFIC_REFERENCES.md",
        "2012AstL...38..331A",
        "10.1051/0004-6361:20078357",
        "10.26093/cds/vizier",
    ),
    "crates/nsb/THIRD_PARTY_NOTICES.md": STELLAR_NOTICE
    + ("2012AstL...38..331A", "10.1051/0004-6361:20078357", "10.26093/cds/vizier"),
}


def check_sources() -> None:
    for path, markers in REQUIRED.items():
        document = (ROOT / path).read_text(encoding="utf-8")
        missing = [marker for marker in markers if marker not in document]
        if missing:
            raise SystemExit(f"{path}: missing scientific credit(s): {missing}")


def starlight_is_bundled() -> bool:
    registry = tomllib.loads(
        (ROOT / "crates/nsb/data/manifest.toml").read_text(encoding="utf-8")
    )
    return any(
        item.get("runtime_embedded", False)
        and "starlight" in item.get("path", "").lower()
        for item in registry.get("assets", [])
    )


def check_archive(path: Path) -> None:
    # For a build without bundled Starlight, the current release has no
    # obligation to carry Starlight-specific attribution in each wheel.
    # On promotion, every artifact that ships executable NSB must carry it.
    if not starlight_is_bundled():
        return

    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            read = archive.read
            members = [name for name in names if name.endswith("THIRD_PARTY_NOTICES.md")]
            texts = [read(name).decode("utf-8") for name in members]
    elif path.name.endswith(".crate") or path.name.endswith(".tar.gz"):
        with tarfile.open(path, "r:gz") as archive:
            members = [m for m in archive.getmembers() if m.isfile() and m.name.endswith("THIRD_PARTY_NOTICES.md")]
            texts = [archive.extractfile(member).read().decode("utf-8") for member in members]
    else:
        raise SystemExit(f"unsupported release artifact: {path}")

    if not texts:
        raise SystemExit(f"{path}: bundled Starlight requires embedded THIRD_PARTY_NOTICES.md")

    if not any(all(marker in text for marker in STELLAR_NOTICE) for text in texts):
        raise SystemExit(
            f"{path}: embedded notice lacks Gaia/XHIP/Hipparcos-2/SVO/CDS credit"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--artifacts",
        type=Path,
        help="Directory with built .crate, .whl and .tar.gz release artifacts",
    )
    args = parser.parse_args()
    check_sources()

    if args.artifacts is not None:
        archives = sorted(
            path
            for path in args.artifacts.rglob("*")
            if path.is_file()
            and (path.suffix in {".crate", ".whl"} or path.name.endswith(".tar.gz"))
        )
        if not archives:
            raise SystemExit(f"no distribution archives found under {args.artifacts}")
        for archive in archives:
            check_archive(archive)
        print(f"Starlight notices: checked {len(archives)} release archive(s)")
    else:
        print("Starlight citations: source notices and bibliographic markers present")


if __name__ == "__main__":
    main()
