#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Reject sdists whose PEP 639 License-File metadata PyPI would reject."""

from email.parser import BytesParser
from pathlib import Path, PurePosixPath
import sys
import tarfile


def verify_sdist(path: Path) -> None:
    if not path.name.endswith(".tar.gz"):
        raise SystemExit(f"expected a .tar.gz sdist, got {path}")

    root = path.name.removesuffix(".tar.gz")
    with tarfile.open(path, "r:gz") as archive:
        try:
            info = archive.extractfile(f"{root}/PKG-INFO")
        except KeyError:
            raise SystemExit(f"{path}: missing {root}/PKG-INFO") from None
        if info is None:
            raise SystemExit(f"{path}: PKG-INFO is not a file")

        metadata = BytesParser().parsebytes(info.read(), headersonly=True)
        license_files = metadata.get_all("License-File", [])
        if "LICENSE" not in license_files:
            raise SystemExit(f"{path}: metadata must declare License-File: LICENSE")

        missing = []
        for relative in license_files:
            rel_path = PurePosixPath(relative)
            if rel_path.is_absolute() or ".." in rel_path.parts or "\\" in relative:
                raise SystemExit(f"{path}: invalid License-File path {relative!r}")
            member_name = f"{root}/{relative}"
            try:
                member = archive.getmember(member_name)
            except KeyError:
                missing.append(relative)
                continue
            if not member.isfile() or member.size == 0:
                missing.append(relative)

        if missing:
            raise SystemExit(
                f"{path}: License-File entries absent or empty in sdist: {missing}. "
                "PyPI rejects these archives (PEP 639)."
            )
        print(f"{path}: verified PEP 639 license files: {license_files}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: check-pypi-sdist-license.py DIST.tar.gz")
    verify_sdist(Path(sys.argv[1]))
