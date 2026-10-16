# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Independent analytic colour law and spatial partition test."""

import hashlib
import json
import subprocess
import sys
from pathlib import Path

import numpy as np


def test_known_colour_law_is_recovered_on_unseen_sky_regions(tmp_path):
    rows = []
    for i, colour in enumerate(np.linspace(-0.2, 3.5, 2000)):
        magnitude = 10 + (i % 100) / 12.5
        unit_flux = np.exp(
            25 + 0.2 * colour - 0.03 * colour**2 - 0.4 * np.log(10) * magnitude
        )
        flux = [7 * unit_flux] + [unit_flux] * 7
        rows.append(
            {
                "source_id": str(i * 4093),
                "pixel": i * 4,
                "g": magnitude,
                "bp": magnitude + 0.6 * colour,
                "rp": magnitude - 0.4 * colour,
                "bp_rp": colour,
                "flux_ph_m2_s": flux,
                "variance": [(x * 0.001) ** 2 for x in flux],
            }
        )
    data = json.dumps(
        {"processed_partitions": 2, "expected_partitions": 2, "samples": rows}
    ).encode()
    source = tmp_path / "samples.json"
    source.write_bytes(data)
    output = tmp_path / "fit.json"
    subprocess.run(
        [
            sys.executable,
            str(Path(__file__).with_name("fit_interval_photometry.py")),
            "--samples",
            str(source),
            "--samples-sha256",
            hashlib.sha256(data).hexdigest(),
            "--output",
            str(output),
        ],
        check=True,
    )
    fitted = json.loads(output.read_text())
    assert fitted["status"] == "research_only_not_runtime_admitted"
    assert fitted["selected_sources"] == 2000
    target = fitted["fits"]["g_bp_rp"]
    assert max(target["heldout_validation"]["test"]["p95_absolute_relative"]) < 1e-10
    assert np.allclose(
        target["parameters"][0], [25 + np.log(7), 0.2, -0.03, 0], atol=1e-10
    )
    assert (
        sum(
            target["heldout_validation"][r]["sources"]
            for r in ["training", "validation", "test"]
        )
        == 2000
    )
