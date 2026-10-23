# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""No-network release automation regressions."""
import hashlib
import importlib.util
import io
import tarfile
import tempfile
import unittest
from pathlib import Path
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release_assets", ROOT / "scripts/release_assets.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.dist = Path(temp.name)

    def artifacts(self, version="0.2.0"):
        platforms = ["manylinux_2_28_x86_64", "manylinux_2_28_aarch64",
                     "macosx_10_12_x86_64", "macosx_11_0_arm64", "win_amd64"]
        for platform in platforms:
            name = f"nsb_py-{version}-cp310-abi3-{platform}.whl"
            with ZipFile(self.dist / name, "w") as z:
                z.writestr(f"nsb_py-{version}.dist-info/METADATA",
                           f"Name: nsb-py\nVersion: {version}\n")
        with tarfile.open(self.dist / f"nsb_py-{version}.tar.gz", "w:gz") as z:
            data = f"Name: nsb-py\nVersion: {version}\n".encode()
            entry = tarfile.TarInfo(f"nsb_py-{version}/PKG-INFO")
            entry.size = len(data)
            z.addfile(entry, io.BytesIO(data))
        return release.verify_artifacts(self.dist, f"v{version}")

    def test_tags_and_versions(self):
        self.assertEqual(release.version_for("v0.2.0"), "0.2.0")
        for tag in ("v0.2", "0.2.0", "v01.0.0", "v1.2.3-rc1", "v1.2.3foo"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.version_for(tag)

    def test_notes_extract_only_exact_section(self):
        doc = "# Changelog\n## [Unreleased]\nWIP\n## [0.2.0]\n\n### Added\n\n- New\n\n## [0.1.1] 15/10/2026\nOld\n"
        self.assertEqual(release.extract_notes(doc, "v0.2.0"), "### Added\n\n- New\n")
        for broken in (doc + "## [0.2.0]\nRepeated\n", "## [0.2.0]\n### Added\n## [0.1.1]\nOld\n",
                       "## [0.2.0\nMalformed\n"):
            with self.subTest(broken=broken), self.assertRaises(ValueError):
                release.extract_notes(broken, "v0.2.0")
        with self.assertRaises(ValueError):
            release.extract_notes(doc, "v0.3.0")

    def test_six_wheels_and_sdist_and_missing_file(self):
        files = self.artifacts()
        self.assertEqual(len(files), 6)
        files[0].unlink()
        with self.assertRaisesRegex(ValueError, "five wheels"):
            release.verify_artifacts(self.dist, "v0.2.0")

    def test_extra_file_rejected(self):
        self.artifacts()
        (self.dist / "unexpected.txt").write_text("unexpected")
        with self.assertRaisesRegex(ValueError, "extra files"):
            release.verify_artifacts(self.dist, "v0.2.0")

    def test_version_mismatch_and_duplicate_platform(self):
        self.artifacts()
        wheel = next(self.dist.glob("*win_amd64.whl"))
        wheel.rename(self.dist / "nsb_py-0.1.1-cp310-abi3-win_amd64.whl")
        with self.assertRaisesRegex(ValueError, "unexpected wheel"):
            release.verify_artifacts(self.dist, "v0.2.0")
        (self.dist / "nsb_py-0.1.1-cp310-abi3-win_amd64.whl").rename(
            self.dist / "nsb_py-0.2.0-cp310-abi3-manylinux_2_35_x86_64.whl")
        with self.assertRaisesRegex(ValueError, "duplicate wheel"):
            release.verify_artifacts(self.dist, "v0.2.0")

    def test_embedded_wheel_and_sdist_metadata(self):
        self.artifacts()
        wheel = next(self.dist.glob("*win_amd64.whl"))
        with ZipFile(wheel, "w") as z:
            z.writestr("nsb_py-0.2.0.dist-info/METADATA", "Name: wrong\nVersion: 0.2.0\n")
        with self.assertRaisesRegex(ValueError, "incorrect wheel metadata"):
            release.verify_artifacts(self.dist, "v0.2.0")
        wheel.unlink()
        self.artifacts()
        sdist = self.dist / "nsb_py-0.2.0.tar.gz"
        with tarfile.open(sdist, "w:gz") as z:
            payload = b"Name: wrong\nVersion: 0.2.0\n"
            entry = tarfile.TarInfo("nsb_py-0.2.0/PKG-INFO")
            entry.size = len(payload)
            z.addfile(entry, io.BytesIO(payload))
        with self.assertRaisesRegex(ValueError, "incorrect sdist metadata"):
            release.verify_artifacts(self.dist, "v0.2.0")

    def test_registry_completion_and_sha256(self):
        files = self.artifacts()
        urls = [{"filename": p.name, "digests": {"sha256": hashlib.sha256(p.read_bytes()).hexdigest()}} for p in files]
        def fetch(url):
            return {"version": {"num": "0.2.0", "yanked": False}} if "crates.io" in url else {"urls": urls}
        release.verify_published(files, "v0.2.0", fetch)
        urls[0]["digests"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            release.verify_published(files, "v0.2.0", fetch)
        urls.pop()
        with self.assertRaisesRegex(ValueError, "inventory mismatch"):
            release.verify_published(files, "v0.2.0", fetch)
        with self.assertRaisesRegex(ValueError, "absent or yanked"):
            release.verify_published(files, "v0.2.0", lambda _: {"version": {"num": "0.2.0", "yanked": True}})

    def test_skipped_jobs_failure_paths_and_release_only(self):
        def ready(event, mode, r, cancelled=False):
            if cancelled or r["scientific"] != "success":
                return False
            if event == "push":
                return r["crates"] == "success" and r["pypi"] == "success"
            return (event == "workflow_dispatch" and r["validate"] == "success" and
                    (r["sdist"] == "success" or (mode == "github-release" and r["sdist"] == "skipped")))
        base = dict(scientific="success", crates="success", pypi="success", validate="skipped", sdist="skipped")
        self.assertTrue(ready("push", "pypi-sdist", base))
        for key in ("scientific", "crates", "pypi"):
            with self.subTest(key=key):
                self.assertFalse(ready("push", "pypi-sdist", {**base, key: "failure"}))
        recovery = {**base, "validate": "success", "crates": "skipped", "pypi": "skipped"}
        self.assertTrue(ready("workflow_dispatch", "pypi-sdist", {**recovery, "sdist": "success"}))
        self.assertTrue(ready("workflow_dispatch", "github-release", recovery))
        self.assertFalse(ready("workflow_dispatch", "pypi-sdist", recovery))
        self.assertFalse(ready("workflow_dispatch", "github-release", {**recovery, "validate": "failure"}))
        self.assertFalse(ready("push", "pypi-sdist", base, cancelled=True))

    def test_workflow_has_explicit_success_gate_and_no_generated_notes(self):
        workflow = (ROOT / ".github/workflows/publish.yml").read_text()
        for fragment in ("!cancelled()", "needs.record-publication.result == 'success'",
                         "needs.publish-crates-io.result == 'success'",
                         "needs.publish-pypi.result == 'success'",
                         "needs.recovery-validate.result == 'success'",
                         "needs.recover-pypi-sdist.result == 'success'",
                         "inputs.recovery_mode == 'github-release'",
                         "name: crates.io", "name: pypi", "id-token: write",
                         "--published", "--notes-file"):
            self.assertIn(fragment, workflow)
        self.assertNotIn("name: crates-io", workflow)
        self.assertNotIn("--generate-notes", workflow)


if __name__ == "__main__":
    unittest.main()
