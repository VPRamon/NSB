"""Capture Python `darknsb` outputs as JSON fixtures for Rust cross-validation.

Run from the repo root with the pinned virtual-env:

    .venv/bin/python tools/capture_golden.py

The script iterates over scenarios, calls the unmodified `darknsb`
implementation, and writes one JSON file per scenario into ``tests/golden/``.

The pin requirements (``numpy<2``, ``scipy<1.14``, ``astropy<6``) are needed
because the Python code relies on ``scipy.interpolate.interp2d`` and
``astropy.coordinates.get_moon`` which were removed in newer releases.
"""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DARKNSB_DIR = ROOT / "darknsb" / "darknsb-main"
GOLDEN_DIR = ROOT / "tests" / "golden"

sys.path.insert(0, str(DARKNSB_DIR))
os.chdir(DARKNSB_DIR)

import numpy as np  # noqa: E402
from NSB_Utils import (  # noqa: E402
    setup_observatory,
    setup_source,
    CalculateZL,
    CalculateSL,
    CalculateAG,
    GetSunposition,
)


SCENARIOS = [
    {
        "id": "ctaos_sgrA_2023-09-04_0148",
        "site": "CTAO-S",
        "obstime": "2023-09-04 01:48:00",
        "source": "SgrA*",
    },
]


def _strip_units(q):
    return float(getattr(q, "value", q))


def capture(scenario: dict) -> dict:
    location, obstime = setup_observatory(scenario["site"], scenario["obstime"])
    source = setup_source(scenario["source"], obstime, location)

    integrated_zl, b_zl, v_zl = CalculateZL(location, obstime, source)
    integrated_sl, b_sl, v_sl = CalculateSL()
    integrated_ag, b_ag, v_ag = CalculateAG(location, obstime, source)

    ecli = source.heliocentrictrueecliptic
    altaz = source.altaz
    lambda_sun = GetSunposition(obstime.mjd)

    b_flux = b_zl + b_sl + b_ag
    v_flux = v_zl + v_sl + v_ag
    v_mag = 27.78 - 2.5 * np.log10(v_flux)
    b_mag = 27.78 - 2.5 * np.log10(b_flux)

    return {
        "scenario": scenario["id"],
        "inputs": scenario,
        "geometry": {
            "site_lat_deg": float(location.lat.deg),
            "site_lon_deg": float(location.lon.deg),
            "site_height_m": float(location.height.value),
            "mjd": float(obstime.mjd),
            "source_alt_deg": float(altaz.alt.deg),
            "source_az_deg": float(altaz.az.deg),
            "source_ecl_lon_deg": float(ecli.lon.deg),
            "source_ecl_lat_deg": float(ecli.lat.deg),
            "sun_lambda_deg": float(getattr(lambda_sun, "deg", lambda_sun)),
        },
        "components": {
            "zodiacal":  {"integrated": _strip_units(integrated_zl),
                          "b_s10": float(b_zl), "v_s10": float(v_zl)},
            "starlight": {"integrated": _strip_units(integrated_sl),
                          "b_s10": float(b_sl), "v_s10": float(v_sl)},
            "airglow":   {"integrated": _strip_units(integrated_ag),
                          "b_s10": float(b_ag), "v_s10": float(v_ag)},
        },
        "totals": {
            "integrated": _strip_units(integrated_zl + integrated_sl + integrated_ag),
            "b_mag": float(b_mag),
            "v_mag": float(v_mag),
        },
    }


def main() -> None:
    GOLDEN_DIR.mkdir(parents=True, exist_ok=True)
    for scenario in SCENARIOS:
        result = capture(scenario)
        out = GOLDEN_DIR / f"{scenario['id']}.json"
        out.write_text(json.dumps(result, indent=2, sort_keys=True))
        print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
