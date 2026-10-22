#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Project only CC-BY-4.0 PALACE data, never import/execute its GPL program.

Requires astropy and numpy. Input: pinned PALACE.zip; output: canonical JSON.
"""
import hashlib
import io
import json
import sys
from pathlib import Path
from zipfile import ZipFile

from astropy.io import fits

ARCHIVE = "a42577ffee8f76d9ac3765a9a2811d766c30158cf74c0a364ebf7a726dd9a0dc"
INPUTS = {
    "palace_lines.fits": "280d45262d6fc14aac5c9e01ea79b96e001eb9d4702bf8272a0e47b9446f96a6",
    "palace_var.fits": "e9c754802d7bd1c6fb1b2c1577b1d6c2b29b86b314e65acfbe482a7711c14234",
}

def project(archive: Path) -> str:
    raw = archive.read_bytes()
    if hashlib.sha256(raw).hexdigest() != ARCHIVE:
        raise ValueError("PALACE archive checksum mismatch")
    tables = {}
    with ZipFile(io.BytesIO(raw)) as z:
        for name, digest in INPUTS.items():
            data = z.read("PALACE/src/palace/data/" + name)
            if hashlib.sha256(data).hexdigest() != digest:
                raise ValueError("PALACE FITS checksum mismatch")
            with fits.open(io.BytesIO(data)) as h:
                tables[name] = h[1].data.copy()
    lines = [r for r in tables['palace_lines.fits']
             if r['isX'] == 1 and r['varID'] in ('Og', 'Or') and r['lam'] < .65]
    if len(lines) != 4:
        raise ValueError("Unexpected oxygen subset")
    product = {
        "schema_version": 1,
        "wavelength_convention": "vacuum_nm",
        "radiance_unit": "R",
        "lines": [{"wavelength_nm": float(r['lam'] * 1000), "intensity_r": float(r['I']),
                   "height_km": float(r['hlayer']), "family": 0 if r['varID'] == 'Og' else 1}
                  for r in lines],
        "climatology": [[[float(r[p + family]) for p in ('rI_', 'SCE_', 'rdI_')]
                         for family in ('Og', 'Or')] for r in tables['palace_var.fits']],
    }
    return json.dumps(product, indent=2, allow_nan=False) + '\n'

if __name__ == '__main__':
    Path(sys.argv[2]).write_text(project(Path(sys.argv[1])), encoding='utf-8')
