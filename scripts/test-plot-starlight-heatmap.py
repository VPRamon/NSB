#!/usr/bin/env python3
"""Focused tests for scripts/plot_starlight_heatmap.py."""

from __future__ import annotations

import hashlib
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("plot_starlight_heatmap.py")
SPEC = importlib.util.spec_from_file_location("plot_starlight_heatmap", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def candidate(rows: str, *, ordering: str = "nested", nside: int = 1) -> str:
    return (
        "# schema=nsb-healpix-starlight-candidate-v5\n"
        f"# nside={nside}\n"
        f"# ordering={ordering}\n"
        "# representation=sparse\n"
        "# flux_quantity=integrated_per_pixel\n"
        "# flux_unit=ph_m-2_s-1\n"
        "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,"
        "systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,"
        "admitted_sources,excluded_sources\n"
        f"{rows}"
    )


class StarlightHeatmapTests(unittest.TestCase):
    def write_map(self, root: Path, body: str) -> Path:
        path = root / "candidate.csv"
        path.write_text(body, encoding="utf-8")
        return path

    def test_loads_sparse_nested_candidate(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = self.write_map(
                Path(tmp),
                candidate(
                    "0,1.5,0.1,0.2,0.3,2,0\n"
                    "11,4.0,0.1,0.2,0.3,1,0\n"
                ),
            )
            data = MODULE.load_starlight_map(path)

        self.assertEqual(data.nside, 1)
        self.assertEqual(data.metadata["ordering"], "nested")
        self.assertEqual(data.pixels, ((0, 1.5), (11, 4.0)))

    def test_rejects_non_nested_ordering(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = self.write_map(
                Path(tmp),
                candidate("0,1.0,0.0,0.0,0.0,1,0\n", ordering="ring"),
            )
            with self.assertRaisesRegex(ValueError, "ordering"):
                MODULE.load_starlight_map(path)

    def test_rejects_unsorted_pixels(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = self.write_map(
                Path(tmp),
                candidate(
                    "5,1.0,0.0,0.0,0.0,1,0\n"
                    "4,2.0,0.0,0.0,0.0,1,0\n"
                ),
            )
            with self.assertRaisesRegex(ValueError, "strictly increasing"):
                MODULE.load_starlight_map(path)

    def test_rejects_pixel_outside_nside_domain(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = self.write_map(
                Path(tmp),
                candidate("12,1.0,0.0,0.0,0.0,1,0\n"),
            )
            with self.assertRaisesRegex(ValueError, "outside nside=1"):
                MODULE.load_starlight_map(path)

    def test_title_contains_map_checksum_prefix(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = self.write_map(
                Path(tmp),
                candidate("0,1.0,0.0,0.0,0.0,1,0\n"),
            )
            data = MODULE.load_starlight_map(path)
            expected = hashlib.sha256(path.read_bytes()).hexdigest()[:8]
            title = MODULE.default_title(path, data)

        self.assertIn("NSB Starlight nside=1", title)
        self.assertIn("Galactic", title)
        self.assertIn(expected, title)


if __name__ == "__main__":
    unittest.main()
