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
sys.path.insert(0, str(SCRIPT.parent))
from calibrate_starlight_bv import bin_for, cells_for, load_policy  # noqa: E402
from prepare_starlight_bv import (  # noqa: E402
    DIAGNOSTIC, GAIA, LABELS, PARENT, by_id, census, check_candidate, csv_rows, csv_write,
    freeze, gaia_parent_rows, read_review, selected,
)


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

    def test_half_open_magnitude_and_colour_boundaries(self):
        policy = {**DIAGNOSTIC, "g_edges": [10, 15, 20],
                  "bp_g_edges": [-1, 0.5, 2], "g_rp_edges": [-1, 0.5, 2]}
        row = dict(zip(LABELS, (1, 256, 15, 15.5, 14.5, -1e-18, 1e-18)))
        [(key, target, scale)] = list(cells_for(row, policy, False))
        self.assertEqual(key, ("full", 1, 1, 1, 1))
        self.assertEqual(target, [-1e-18, 1e-18])
        self.assertEqual(scale, 1)
        self.assertEqual(bin_for(policy["g_edges"], 10), 0)
        self.assertEqual(list(cells_for(dict(row, g=20), policy, False)), [])

    def test_workflow_uses_blank_photometry_and_freezes_input_bytes(self):
        rows = [dict(zip(GAIA, values)) for values in
                [(10, 30, 40, 15, "", 14), (20, 50, 60, 19, "", "")]]
        csv_write(self.references, GAIA, rows)
        raw = self.references.read_bytes()
        match = by_id(csv_rows(self.references, GAIA))
        self.assertEqual(match[10]["phot_bp_mean_mag"], "")
        self.assertEqual(match[20]["phot_rp_mean_mag"], "")
        comparison = census(dict(row, duplicated_source="False", b="5",
                                 has_xp_continuous="False", phot_bp_n_blended_transits="",
                                 phot_rp_n_blended_transits="1" if key == 10 else "")
                            for key, row in match.items())
        self.assertEqual({(row["route"], row["sources"]) for row in comparison},
                         {("g_and_rp", 1), ("g_only", 1)})
        self.assertTrue(selected(dict(match[10], duplicated_source="False", has_xp_continuous="True")))
        self.assertTrue(selected(dict(match[10], duplicated_source="false", has_xp_continuous="true")))
        self.assertFalse(selected(dict(match[10], duplicated_source="True", has_xp_continuous="True")))
        csv_write(self.references, GAIA, rows)
        self.assertEqual(self.references.read_bytes(), raw)
        with self.assertRaisesRegex(ValueError, "duplicate Gaia source ID"):
            by_id([rows[0], rows[0]])
        freeze(self.references, raw)
        with self.assertRaisesRegex(ValueError, "differs from frozen"):
            freeze(self.references, b"edited")

    def test_diagnostic_policy_has_no_candidate_provenance(self):
        self.policy.write_text(json.dumps({**DIAGNOSTIC, "software_commit": "a" * 40,
                                           "xp_design_sha256": "b" * 64,
                                           "reference_provenance": "diagnostic fixture"}))
        load_policy(self.policy, exporting=False)
        with self.assertRaisesRegex(ValueError, "candidate export requires"):
            load_policy(self.policy, exporting=True)
        self.assertIn("candidate export requires",
                      self.command("--candidate-calibration", self.model, succeeds=False))

    def test_bulk_null_photometry_is_not_a_measured_colour(self):
        row = dict.fromkeys(PARENT, "null")
        row.update(source_id="10", duplicated_source="False", has_xp_continuous="False",
                   phot_g_mean_mag="19", ra="0", dec="0", l="5", b="5", random_index="9")
        csv_write(self.references, PARENT, [row])
        [normalized] = gaia_parent_rows(self.references)
        self.assertEqual(normalized["phot_bp_mean_mag"], "")
        self.assertEqual(normalized["phot_rp_mean_mag"], "")
        self.assertEqual(census([normalized])[0]["route"], "g_only")

    def test_independent_review_checks_pins_and_held_out_support(self):
        self.write_sources()
        self.command()
        policy = json.loads(self.policy.read_text())
        report = json.loads(self.report.read_text())
        self.model.write_text(json.dumps({"required_cells": report["cells"]}))
        review = self.policy.with_name("review.json")
        reference_pin = {"path": self.references.name,
                         "sha256": hashlib.sha256(self.references.read_bytes()).hexdigest()}
        target_pin = {"path": self.model.name, "sha256": hashlib.sha256(self.model.read_bytes()).hexdigest()}
        payload = {name: reference_pin for name in (
            "gaia_source", "spectral_reference", "xp_absolute_calibration_evidence",
            "reference_evidence",
        )}
        payload.update(
            target_population_evidence=target_pin,
            reviewer="synthetic fixture",
            selection_and_crossmatch="synthetic fixture",
            wavelength_operator="synthetic fixture",
            uncertainty_model="synthetic fixture",
            measurement={"vacuum_nm": [445, 551], "unit": "W m-2 nm-1"},
            reference_independent_of_gaia_xp_and_photometry=True,
            policy={key: value for key, value in policy.items()
                    if key not in ("software_commit", "xp_design_sha256", "reference_provenance")},
            acceptance={"min_test_per_cell": 2, "max_abs_bias": 0.1,
                        "max_p95_absolute_relative_error": 1,
                        "min_coverage_68": 0.5, "min_coverage_95": 0.95},
        )
        review.write_text(json.dumps(payload))
        files, thresholds = read_review(review, {})
        check_candidate(report, thresholds, files["target_population_evidence"])
        with self.assertRaisesRegex(ValueError, "insufficient reference support"):
            check_candidate(report, dict(thresholds, min_test_per_cell=4), self.model)
        report["cells"][0]["bands_b_v"][0]["relative_bias_sum"] = 0.2
        with self.assertRaisesRegex(ValueError, "held-out acceptance failed"):
            check_candidate(report, thresholds, self.model)
        payload["reference_independent_of_gaia_xp_and_photometry"] = False
        review.write_text(json.dumps(payload))
        with self.assertRaisesRegex(ValueError, "requires independent"):
            read_review(review, {})
        self.references.write_text("corrupted reference")
        with self.assertRaisesRegex(ValueError, "sha256 mismatch"):
            read_review(review, {})


if __name__ == "__main__":
    unittest.main()
