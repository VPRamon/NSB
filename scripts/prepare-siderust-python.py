#!/usr/bin/env python3
"""Fetch the checksum-pinned siderust Python source crate for wheel tests."""

from __future__ import annotations

import hashlib
import io
from pathlib import Path
import tarfile
import urllib.request


VERSION = "0.2.0"
SHA256 = "a1439bd0885d0ad58b1a33f1db58f4462d34dd2da734ba725d27eda54c181036"
URL = f"https://static.crates.io/crates/siderust-py/siderust-py-{VERSION}.crate"
DESTINATION = Path(".siderust-py")
ARCHIVE_ROOT = f"siderust-py-{VERSION}"


def main() -> None:
    if DESTINATION.exists():
        raise SystemExit(f"refusing to overwrite {DESTINATION}")

    with urllib.request.urlopen(URL) as response:
        archive = response.read()
    actual = hashlib.sha256(archive).hexdigest()
    if actual != SHA256:
        raise SystemExit(f"siderust-py crate checksum mismatch: expected {SHA256}, got {actual}")

    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as package:
        for member in package.getmembers():
            parts = Path(member.name).parts
            if not parts or parts[0] != ARCHIVE_ROOT or ".." in parts:
                raise SystemExit(f"unsafe siderust-py crate member: {member.name}")
            relative = Path(*parts[1:])
            if not relative.parts:
                continue
            target = DESTINATION / relative
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                target.parent.mkdir(parents=True, exist_ok=True)
                source = package.extractfile(member)
                if source is None:
                    raise SystemExit(f"could not read siderust-py crate member: {member.name}")
                target.write_bytes(source.read())
            else:
                raise SystemExit(f"unsupported siderust-py crate member: {member.name}")


if __name__ == "__main__":
    main()
