#!/usr/bin/env python3
"""Unit tests for generate_starlight_heatmap.py ordering and frame semantics."""

from __future__ import annotations

import csv
import tempfile
import unittest
from pathlib import Path

import healpy as hp
import numpy as np

from generate_starlight_heatmap import coordinate_frame_to_healpy, load_quantity


class HeatmapRendererTests(unittest.TestCase):
    def write_map(self, header: str, rows: list[tuple[int, float, int]]) -> Path:
        path = Path(tempfile.mkdtemp()) / "map.csv"
        nside = 1
        npix = 12
        if "nside=128" in header:
            nside = 128
            npix = 12 * 128 * 128
        dense = "representation=sparse" not in header and len(rows) < npix
        with path.open("w", encoding="utf-8", newline="") as handle:
            handle.write(header)
            handle.write(
                "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,"
                "systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,"
                "admitted_sources,excluded_sources\n"
            )
            writer = csv.writer(handle)
            if dense:
                occupied = {pixel: (flux, admitted) for pixel, flux, admitted in rows}
                for pixel in range(npix):
                    flux, admitted = occupied.get(pixel, (0.0, 0))
                    writer.writerow([pixel, flux, 0.0, 0.0, 0.0, admitted, 0])
            else:
                for pixel, flux, admitted in rows:
                    writer.writerow([pixel, flux, 0.0, 0.0, 0.0, admitted, 0])
        return path

    def test_nested_galactic_pixel_is_not_reordered(self) -> None:
        header = (
            "# schema=nsb-healpix-starlight-candidate-v5\n"
            "# map_type=healpix\n"
            "# coordinate_frame=galactic\n"
            "# ordering=nested\n"
            "# nside=1\n"
        )
        path = self.write_map(header, [(0, 10.0, 1), (1, 20.0, 1)])
        values, metadata = load_quantity(path, "flux")
        self.assertEqual(metadata["nest"], "true")
        self.assertEqual(values[0], 10.0)
        self.assertEqual(values[1], 20.0)

    def test_ring_map_keeps_ring_indexing(self) -> None:
        header = (
            "# schema=nsb-healpix-starlight-candidate-v5\n"
            "# map_type=healpix\n"
            "# coordinate_frame=galactic\n"
            "# ordering=ring\n"
            "# nside=1\n"
        )
        path = self.write_map(header, [(0, 30.0, 1), (5, 40.0, 1)])
        values, metadata = load_quantity(path, "flux")
        self.assertEqual(metadata["nest"], "false")
        self.assertEqual(values[0], 30.0)
        self.assertEqual(values[5], 40.0)
        self.assertEqual(values[1], 0.0)

    def test_unknown_frame_fails_closed(self) -> None:
        with self.assertRaises(ValueError):
            coordinate_frame_to_healpy("supergalactic")

    def test_equatorial_frame_maps_to_healpy_c(self) -> None:
        self.assertEqual(coordinate_frame_to_healpy("equatorial"), "C")


if __name__ == "__main__":
    unittest.main()
