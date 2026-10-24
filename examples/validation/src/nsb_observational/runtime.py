# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Explicit installation commands and build-to-import identity verification."""

from __future__ import annotations

import argparse
import hashlib
import importlib
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from importlib import metadata
from pathlib import Path
from urllib.request import urlopen

from .data import sha256

NSB2_COMMIT = "bc9320db03fbff69997ce366c6e76a37339c5d00"


def command(*args, cwd=None, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env, text=True).strip()


def source_identity(source, commit):
    source = Path(source).resolve()
    if not re.fullmatch("[0-9a-f]{40}", commit):
        raise ValueError("Specify a full exact 40-character Git commit")
    if command("git", "rev-parse", "HEAD", cwd=source) != commit:
        raise ValueError("Source HEAD differs from configured commit")
    if command("git", "status", "--porcelain", "--untracked-files=all", cwd=source):
        raise ValueError("Source must be clean, including untracked files")
    lock = tomllib.loads((source / "Cargo.lock").read_text())
    providers = {
        p["name"]: p["version"]
        for p in lock["package"]
        if p["name"] in ("siderust", "tempoch", "siderust-py", "tempoch-py")
    }
    return {
        "commit": commit,
        "source_clean": True,
        "cargo_lock_sha256": sha256(source / "Cargo.lock"),
        "rust_providers": providers,
    }


def wheel_files(wheel):
    with zipfile.ZipFile(wheel) as archive:
        return {
            p: hashlib.sha256(archive.read(p)).hexdigest()
            for p in archive.namelist()
            if not p.endswith("/") and ".dist-info/" not in p
        }


def capabilities(nsb):
    required = ("PARANAL_PALACE_V1", "PARANAL_PALACE_OXYGEN_V1")
    if (
        not hasattr(nsb, "AirglowModel")
        or any(not hasattr(nsb.AirglowModel, name) for name in required)
        or not hasattr(nsb.NsbEvaluator, "integrate_band_components")
    ):
        raise RuntimeError(
            "Installed nsb-py lacks required Oxygen/restricted-band API. "
            "Use the documented source build; no replacement model is provided."
        )


def verify_runtime(receipt, *, mode, commit=None, package_version=None):
    """Match imported package AND native binary to an explicit wheel receipt."""
    receipt = Path(receipt)
    info = json.loads(receipt.read_text())
    if info["mode"] != mode:
        raise ValueError("Configured runtime mode differs from receipt")
    if mode == "source":
        if not commit or info["commit"] != commit or not info["source_clean"]:
            raise ValueError("Source identity mismatch")
    elif mode == "published":
        if not package_version or info["version"] != package_version:
            raise ValueError("Published version mismatch")
    else:
        raise ValueError("Unknown runtime mode")
    if sha256(info["wheel"]) != info["wheel_sha256"]:
        raise ValueError("Build wheel is missing or modified; retain it with the receipt")
    expected = wheel_files(info["wheel"])
    if expected != info["installed_files"]:
        raise ValueError("Wheel receipt content mismatch")
    dist = metadata.distribution("nsb-py")
    nsb = importlib.import_module("nsb")
    paths = set()
    for name, digest in expected.items():
        path = Path(dist.locate_file(name)).resolve()
        if not path.is_file() or sha256(path) != digest:
            raise ValueError(f"Installed NSB differs from built wheel: {name}")
        paths.add(path)
    native = [p for p in paths if p.suffix in (".so", ".pyd")]
    imported_paths = {
        Path(m.__file__).resolve()
        for name, m in list(sys.modules.items())
        if (name == "nsb" or name.startswith("nsb.")) and getattr(m, "__file__", None)
    }
    if (
        Path(nsb.__file__).resolve() not in paths
        or not native
        or not set(native).issubset(imported_paths)
    ):
        raise ValueError("Imported nsb/native extension is not the receipt-verified installation")
    if metadata.version("nsb-py") != info["version"]:
        raise ValueError("Distribution version differs from receipt")
    capabilities(nsb)
    return {
        **info,
        "receipt_sha256": sha256(receipt),
        "native_extension_sha256": {p.name: sha256(p) for p in native},
        "python_runtime": sys.version,
        "siderust": metadata.version("siderust"),
        "tempoch": metadata.version("tempoch"),
        "rust_siderust": str(getattr(nsb, "SIDERUST_VERSION", "unavailable")),
        "rust_tempoch": str(getattr(nsb, "TEMPOCH_VERSION", "unavailable")),
    }


def verify_nsb2():
    """PEP 610 VCS pin plus installed RECORD hashes and imported-path check."""
    import base64
    import csv
    import io

    import nsb2

    dist = metadata.distribution("nsb2")
    direct = json.loads(dist.read_text("direct_url.json") or "{}")
    if (
        direct.get("vcs_info", {}).get("commit_id") != NSB2_COMMIT
        or direct.get("url") != "https://github.com/GerritRo/nsb2.git"
    ):
        raise ValueError("nsb2 must be installed from the registered HTTPS Git revision")
    if Path(nsb2.__file__).resolve() != Path(dist.locate_file("nsb2/__init__.py")).resolve():
        raise ValueError("nsb2 import shadowed by another module")
    hashes = {}
    for name, digest, _size in csv.reader(io.StringIO(dist.read_text("RECORD"))):
        if name.startswith("nsb2/") and digest:
            algorithm, encoded = digest.split("=", 1)
            if algorithm != "sha256":
                raise ValueError("Unsupported nsb2 RECORD hash")
            expected = base64.urlsafe_b64decode(encoded + "==").hex()
            if sha256(dist.locate_file(name)) != expected:
                raise ValueError(f"Modified nsb2 installation: {name}")
            hashes[name] = expected
    if not hashes:
        raise ValueError("No verifiable nsb2 package records")
    return {"commit": NSB2_COMMIT, "version": dist.version, "direct_url": direct, "files": hashes}


def prepare(args):
    if sys.prefix == sys.base_prefix:
        raise RuntimeError("Run in the isolated notebook virtual environment")
    receipt = Path(args.receipt).resolve()
    if receipt.exists():
        raise FileExistsError(receipt)
    identity = {"mode": args.mode, "python": sys.version, "platform": platform.platform()}
    wheel_dir = Path(args.wheel_dir).resolve()
    wheel_dir.mkdir(parents=True, exist_ok=True)
    if args.mode == "source":
        before = source_identity(args.source, args.commit)
        identity.update(before)
        identity["rustc"] = command("rustc", "-Vv")
        identity["maturin"] = command(sys.executable, "-m", "maturin", "--version")
        with tempfile.TemporaryDirectory() as tmp:
            env = {**os.environ, "PYO3_PYTHON": sys.executable}
            subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "maturin",
                    "build",
                    "--release",
                    "--locked",
                    "--interpreter",
                    sys.executable,
                    "--out",
                    tmp,
                ],
                cwd=args.source,
                env=env,
                check=True,
            )
            if source_identity(args.source, args.commit) != before:
                raise ValueError("Source changed during build")
            wheels = list(Path(tmp).glob("*.whl"))
            if len(wheels) != 1:
                raise ValueError("Expected exactly one built wheel")
            wheel = wheel_dir / wheels[0].name
            with wheel.open("xb") as out:
                out.write(wheels[0].read_bytes())
    else:
        # Resolve an actual PyPI artifact before installation; never infer publication from Git.
        if not args.version:
            raise ValueError("Published mode requires an explicit version")
        with urlopen(f"https://pypi.org/pypi/nsb-py/{args.version}/json", timeout=30) as response:
            registry = json.load(response)
        with tempfile.TemporaryDirectory() as tmp:
            # pip's platform tag selection is used only in this explicit setup command.
            subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "pip",
                    "download",
                    "--only-binary=:all:",
                    "--no-deps",
                    "--index-url",
                    "https://pypi.org/simple",
                    "--dest",
                    tmp,
                    f"nsb-py=={args.version}",
                ],
                check=True,
            )
            files = list(Path(tmp).glob("*.whl"))
            if len(files) != 1:
                raise ValueError("No compatible published wheel")
            wheel = wheel_dir / files[0].name
            registered = next(f for f in registry["urls"] if f["filename"] == wheel.name)
            if sha256(files[0]) != registered["digests"]["sha256"]:
                raise ValueError("Published wheel SHA mismatch")
            with wheel.open("xb") as out:
                out.write(files[0].read_bytes())
            identity.update(
                registry_url=f"https://pypi.org/pypi/nsb-py/{args.version}/json",
                registry_wheel_sha256=registered["digests"]["sha256"],
            )
    subprocess.run(
        [
            "uv",
            "pip",
            "install",
            "--python",
            sys.executable,
            "--no-deps",
            "--reinstall",
            str(wheel),
        ],
        check=True,
    )
    nsb = importlib.import_module("nsb")
    capabilities(nsb)
    identity.update(
        wheel=str(wheel),
        wheel_sha256=sha256(wheel),
        installed_files=wheel_files(wheel),
        version=metadata.version("nsb-py"),
    )
    receipt.parent.mkdir(parents=True, exist_ok=True)
    with receipt.open("x") as out:
        json.dump(identity, out, indent=2, allow_nan=False)
    verify_runtime(
        receipt,
        mode=args.mode,
        commit=getattr(args, "commit", None),
        package_version=getattr(args, "version", None),
    )
    print(f"Verified Oxygen-capable runtime receipt: {receipt}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    for name in ("source", "published"):
        p = sub.add_parser(name)
        p.add_argument("--receipt", required=True)
        p.add_argument("--wheel-dir", required=True)
        if name == "source":
            p.add_argument("--source", type=Path, required=True)
            p.add_argument("--commit", required=True)
        else:
            p.add_argument("--version", required=True)
    prepare(parser.parse_args())


if __name__ == "__main__":
    main()
