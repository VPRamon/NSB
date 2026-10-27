# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Release preflight rejects dependency and version drift without network access."""
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class ReleaseMetadataTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        for name in ["Cargo.toml", "Cargo.lock", "pyproject.toml", "README.md", "README_PYPI.md", "CHANGELOG.md",
                     "scripts/check-release-metadata.py", ".github/workflows/publish.yml",
                     "crates/nsb/Cargo.toml", "crates/nsb-cli/Cargo.toml",
                     "crates/nsb-data-tools/Cargo.toml", "crates/nsb/api/API_FROZEN",
                     "crates/nsb/api/public-api.txt"]:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, target)

    def run_preflight(self, *args):
        return subprocess.run([sys.executable, str(self.root / "scripts/check-release-metadata.py"),
                               *args], capture_output=True, text=True)

    def replace(self, path, old, new):
        target = self.root / path
        content = target.read_text()
        self.assertIn(old, content)
        target.write_text(content.replace(old, new, 1))

    def test_current_registry_release_and_tag_pass(self):
        for args in [(), ("--tag", "v0.2.0")]:
            result = self.run_preflight(*args)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_wrong_tag_and_workspace_version_fail(self):
        self.assertNotEqual(self.run_preflight("--tag", "v0.1.1").returncode, 0)
        self.replace("crates/nsb-cli/Cargo.toml", 'version = "0.2.0"', 'version = "0.1.1"')
        self.assertNotEqual(self.run_preflight().returncode, 0)

    def test_old_python_bridge_requirements_fail(self):
        self.replace("pyproject.toml", "siderust>=0.6.0,<0.7", "siderust>=0.5.1,<0.6")
        self.assertNotEqual(self.run_preflight().returncode, 0)

    def test_unsafe_rust_bridge_requirement_fails(self):
        self.replace("crates/nsb/Cargo.toml", 'siderust-py = { version = "0.6.0"',
                     'siderust-py = { version = "0.5.1"')
        result = self.run_preflight()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("bridge protocol 2", result.stderr)

    def test_unsafe_locked_rust_bridge_fails(self):
        self.replace("Cargo.lock", 'name = "siderust-py"\nversion = "0.6.0"',
                     'name = "siderust-py"\nversion = "0.5.1"')
        result = self.run_preflight()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("0.6 bridge protocol", result.stderr)

    def test_dependency_patch_fails_even_without_tag(self):
        with (self.root / "Cargo.toml").open("a") as stream:
            stream.write('\n[patch.crates-io]\nsiderust = { path = "vendor/siderust" }\n')
        self.assertNotEqual(self.run_preflight().returncode, 0)

    def test_git_source_and_missing_checksum_fail(self):
        lock = self.root / "Cargo.lock"
        original = lock.read_text()
        self.replace("Cargo.lock", 'source = "registry+https://github.com/rust-lang/crates.io-index"',
                     'source = "git+https://example.invalid/provider#' + "a" * 40 + '"')
        self.assertNotEqual(self.run_preflight().returncode, 0)
        lock.write_text(original)
        checksum = next(line for line in original.splitlines() if line.startswith("checksum = "))
        self.replace("Cargo.lock", checksum, checksum.replace('"', '"bad', 1))
        self.assertNotEqual(self.run_preflight().returncode, 0)

    def test_duplicate_provider_fails(self):
        with (self.root / "Cargo.lock").open("a") as stream:
            stream.write('\n[[package]]\nname = "siderust"\nversion = "0.15.0"\n'
                         'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
                         'checksum = "' + "a" * 64 + '"\n')
        result = self.run_preflight()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exactly one siderust", result.stderr)


if __name__ == "__main__":
    unittest.main()
