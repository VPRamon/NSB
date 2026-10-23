#!/usr/bin/env python3
"""Fail-closed NSB GitHub Release notes and Python artifact verification."""

import argparse
import hashlib
import json
import re
import tarfile
from email.parser import BytesParser
from pathlib import Path
from urllib.request import Request, urlopen
from zipfile import ZipFile

TAG_PATTERN = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
SECTION_PATTERN = re.compile(r"^## \[([^\]]+)\](?:[ \t]+[^\n]*)?$", re.MULTILINE)
TARGETS = {"linux-x86_64", "linux-aarch64", "macos-x86_64", "macos-arm64", "windows-amd64"}


def version_for(tag):
    match = TAG_PATTERN.fullmatch(tag)
    if match is None:
        raise ValueError(f"invalid release tag: {tag!r}")
    return ".".join(match.groups())


def extract_notes(changelog, tag):
    version = version_for(tag)
    sections = list(SECTION_PATTERN.finditer(changelog))
    matches = [i for i, match in enumerate(sections) if match.group(1) == version]
    if len(matches) != 1:
        raise ValueError(f"expected exactly one changelog section for {version}; found {len(matches)}")
    idx = matches[0]
    start = sections[idx].end()
    end = sections[idx + 1].start() if idx + 1 < len(sections) else len(changelog)
    text = changelog[start:end].strip()
    if not any(line.strip() and not line.lstrip().startswith("#") for line in text.splitlines()):
        raise ValueError(f"empty changelog section for {version}")
    return text + "\n"


def _platform(filename):
    suffix = filename.rsplit("-", 1)[-1]
    if not suffix.endswith(".whl"):
        raise ValueError(f"invalid wheel extension: {filename}")
    part = suffix[:-4]
    if part.startswith("manylinux_") and part.endswith("_x86_64"):
        return "linux-x86_64"
    if part.startswith("manylinux_") and part.endswith("_aarch64"):
        return "linux-aarch64"
    if part.startswith("macosx_") and part.endswith("_x86_64"):
        return "macos-x86_64"
    if part.startswith("macosx_") and part.endswith("_arm64"):
        return "macos-arm64"
    if part == "win_amd64":
        return "windows-amd64"
    raise ValueError(f"unrecognized platform: {filename}")


def verify_artifacts(directory, tag):
    version = version_for(tag)
    files = sorted(Path(directory).iterdir())
    wheels = [p for p in files if p.suffix == ".whl"]
    sdists = [p for p in files if p.name.endswith(".tar.gz")]
    if len(files) != 6 or len(wheels) != 5 or len(sdists) != 1 or any(not p.is_file() for p in files):
        raise ValueError("expected exactly five wheels and one sdist, without extra files")
    targets = set()
    for wheel in wheels:
        if not wheel.name.startswith(f"nsb_py-{version}-cp310-abi3-"):
            raise ValueError(f"unexpected wheel tag/version: {wheel.name}")
        target = _platform(wheel.name)
        if target in targets:
            raise ValueError(f"duplicate wheel target: {target}")
        targets.add(target)
        with ZipFile(wheel) as archive:
            members = [n for n in archive.namelist() if n.endswith(".dist-info/METADATA")]
            if len(members) != 1 or not members[0].startswith(f"nsb_py-{version}.dist-info/"):
                raise ValueError(f"missing/ambiguous wheel metadata: {wheel.name}")
            metadata = BytesParser().parsebytes(archive.read(members[0]))
            if metadata.get("Name") != "nsb-py" or metadata.get("Version") != version:
                raise ValueError(f"incorrect wheel metadata: {wheel.name}")
    if targets != TARGETS:
        raise ValueError(f"missing wheel target(s): {sorted(TARGETS - targets)}")
    sdist = sdists[0]
    if sdist.name != f"nsb_py-{version}.tar.gz":
        raise ValueError(f"unexpected sdist name: {sdist.name}")
    with tarfile.open(sdist, "r:gz") as archive:
        member = archive.extractfile(f"nsb_py-{version}/PKG-INFO")
        if member is None:
            raise ValueError("missing sdist PKG-INFO")
        metadata = BytesParser().parsebytes(member.read())
        if metadata.get("Name") != "nsb-py" or metadata.get("Version") != version:
            raise ValueError("incorrect sdist metadata")
    return files


def _get_json(url):
    request = Request(url, headers={"User-Agent": "NSB-release-verifier (https://github.com/VPRamon/NSB)"})
    with urlopen(request, timeout=30) as response:
        return json.load(response)


def verify_published(files, tag, get_json=_get_json):
    """Require matching registry version and the exact six already-published bytes."""
    version = version_for(tag)
    crate = get_json(f"https://crates.io/api/v1/crates/nsb/{version}")
    if crate.get("version", {}).get("num") != version or crate["version"].get("yanked"):
        raise ValueError(f"nsb {version} is absent or yanked on crates.io")
    data = get_json(f"https://pypi.org/pypi/nsb-py/{version}/json")
    published = data["urls"]
    expected = {p.name for p in files}
    names = [entry["filename"] for entry in published]
    if len(names) != len(set(names)) or set(names) != expected:
        raise ValueError(f"PyPI artifact inventory mismatch: {set(names) ^ expected}")
    by_name = {entry["filename"]: entry for entry in published}
    for file in files:
        digest = hashlib.sha256(file.read_bytes()).hexdigest()
        if digest != by_name[file.name]["digests"]["sha256"]:
            raise ValueError(f"PyPI SHA-256 mismatch: {file.name}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    notes = commands.add_parser("notes")
    notes.add_argument("--tag", required=True)
    notes.add_argument("--changelog", type=Path, required=True)
    notes.add_argument("--output", type=Path, required=True)
    verify = commands.add_parser("verify")
    verify.add_argument("--tag", required=True)
    verify.add_argument("--dist", type=Path, required=True)
    verify.add_argument("--published", action="store_true")
    args = parser.parse_args(argv)
    if args.command == "notes":
        args.output.write_text(extract_notes(args.changelog.read_text(encoding="utf-8"), args.tag), encoding="utf-8")
    else:
        files = verify_artifacts(args.dist, args.tag)
        if args.published:
            verify_published(files, args.tag)
        print(f"Verified {len(files)} distribution artifacts for {args.tag}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, tarfile.TarError) as error:
        raise SystemExit(f"release validation failed: {error}") from error
