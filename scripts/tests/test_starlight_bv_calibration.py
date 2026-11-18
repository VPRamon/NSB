# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Exercise deterministic fitting with labelled synthetic populations."""

import csv
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / "crates/nsb-data-tools/tools/calibrate_starlight_bv.py"


class StarlightBvCalibration(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        self.references = root / "labels.csv"
        self.policy = root / "policy.json"
        self.report = root / "held-out.json"
        self.model = root / "candidate.json"
        self.policy.write_text(json.dumps({
            "seed": 0, "sky_nside": 1, "g_edges": [14, 16],
            "bp_g_edges": [-1, 2], "g_rp_edges": [-1, 2],
            "min_training": 2, "min_validation": 2, "min_validation_sky_cells": 2,
            "software_commit": "a" * 40, "xp_design_sha256": "b" * 64,
            "xp_shared_calibration_fraction": [0.01, 0.02],
            "reference_provenance": "synthetic fixture only"
        }))
        self.pixels = [[], [], []]
        for pixel in range(256):
            bucket = int(hashlib.sha256(f"0:{pixel}".encode()).hexdigest()[:16], 16) % 10
            self.pixels[0 if bucket < 6 else 1 if bucket < 8 else 2].append(pixel)

    def write_sources(self, *, test_factor=1, full_only=False, identical_validation=False):
        with self.references.open("w", newline="") as output:
            writer = csv.writer(output)
            writer.writerow(("source_id", "galactic_nest16", "g", "bp", "rp",
                             "b_w_m2_nm", "v_w_m2_nm"))
            source = 0
            for bp, rp in [(16, 14)] if full_only else [(16, 14), (16, ""), ("", 14), ("", "")]:
                for split, pixels in enumerate(self.pixels):
                    for offset, pixel in enumerate(pixels[:3]):
                        source += 1
                        if identical_validation and split == 1:
                            pixel = pixels[0]
                        factor = test_factor if split == 2 else 1
                        writer.writerow((source, pixel, 15, bp, rp,
                                         (offset + 1) * 1e-14 * factor,
                                         (3 - offset) * 2e-14 * factor))

    def command(self, *extra, succeeds=True):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), str(self.references), str(self.policy),
             str(self.report), *map(str, extra)],
            text=True, capture_output=True, check=False,
        )
        self.assertEqual(result.returncode == 0, succeeds, result.stderr)
        return result.stderr

    def test_disjoint_held_out_rows_do_not_change_the_model(self):
        self.write_sources()
        self.command("--candidate-calibration", self.model)
        first_model, first_report = self.model.read_bytes(), self.report.read_bytes()
        self.command("--candidate-calibration", self.model)
        self.assertEqual(first_model, self.model.read_bytes())
        self.assertEqual(first_report, self.report.read_bytes())
        model = json.loads(first_model)
        report = json.loads(first_report)
        self.assertEqual(model["held_out_report_sha256"], hashlib.sha256(first_report).hexdigest())
        self.assertEqual(model["policy_sha256"], hashlib.sha256(self.policy.read_bytes()).hexdigest())
        self.assertEqual(len(model["cells"]), 4)
        self.assertTrue(all(cell["residual_covariance_at_g15"][2] < 0 for cell in model["cells"]))
        self.assertEqual({entry["evaluated"] for entry in report["regions"]}, {3})
        self.assertEqual(len(report["cells"]), 4)
        self.assertTrue(all(cell["fitted"] and cell["evaluated"] == 3 for cell in report["cells"]))
        self.assertEqual({cell["g_bin"] for cell in report["cells"]}, {0})
        cells = [set(report["splits"][split]["sky_regions"]) for split in ("training", "validation", "test")]
        self.assertEqual(sum(map(len, cells)), len(set.union(*cells)))

        self.write_sources(test_factor=10)
        self.command("--candidate-calibration", self.model)
        self.assertEqual(json.loads(self.model.read_bytes())["cells"], model["cells"])
        self.assertNotEqual(json.loads(self.report.read_bytes())["regions"], report["regions"])

    def test_missing_routes_and_insufficient_sky_support_never_export(self):
        for options in ({"full_only": True}, {"identical_validation": True}):
            self.write_sources(**options)
            stderr = self.command("--candidate-calibration", self.model, succeeds=False)
            self.assertIn("insufficient training, validation or held-out test support", stderr)
            self.assertFalse(self.model.exists())
            self.assertTrue(self.report.exists())
        self.command("--stress-test-mask-complete")
        self.assertTrue(json.loads(self.report.read_bytes())["masked_stress_test"])
        stderr = self.command("--stress-test-mask-complete", "--candidate-calibration",
                              self.model, succeeds=False)
        self.assertIn("for stress tests", stderr)

    def test_invalid_duplicate_and_non_finite_rows_are_rejected(self):
        self.write_sources()
        raw = self.references.read_bytes()
        for row, error in [
            (raw.splitlines()[1] + b"\n", "duplicate/invalid source"),
            (b"100,0,NaN,,,1,2\n", "non-finite Gaia photometry"),
            (b"100,0,15,,,1,inf\n", "non-finite B/V reference"),
            (b"100,0,15,,,1\n", "incomplete or extra CSV fields"),
        ]:
            self.references.write_bytes(raw + row)
            self.assertIn(error, self.command(succeeds=False))


if __name__ == "__main__":
    unittest.main()
