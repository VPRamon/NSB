"""NOAA/NCEI IQSY Abastumani OI photometer products and equivalence assessment.

These are public historical zenith intensities in Rayleighs. They are useful for
methodology and order-of-magnitude oxygen-line comparisons, but absolute admission
requires photometer passband, absolute calibration uncertainty, F10.7 and domain
compatibility that the IQSY files alone do not certify.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

from .rayleigh import photons_per_ns_sr_from_rayleigh

ABASTUMANI = {
    "name": "Abastumani Astrophysical Observatory",
    "lat_deg": 41.75,
    "lon_deg": 42.82,
    "height_m": 1580.0,
    "source_base": "https://www.ngdc.noaa.gov/stp/space-weather/aurora-airglow/IQSYDATA/",
}


@dataclass(frozen=True)
class HourlyOI:
    station: str
    wavelength_air_angstrom: int
    year: int
    month: int
    day: int
    local_hour: int
    intensity_rayleigh: float


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_iqsy_hourly(path: Path, *, wavelength: int) -> list[HourlyOI]:
    """Parse ABAS/station hourly rows from NOAA IQSY .int files."""
    text = path.read_text(encoding="latin-1")
    rows: list[HourlyOI] = []
    # Example: " ABAS 5577A  64  1  6                     115 124                         120   5 ..."
    pattern = re.compile(
        r"^\s*([A-Z]{3,4})\s+(\d{4})A\s+(\d{1,2})\s+(\d{1,2})\s+(\d{1,2})\s+(.*?)"
        r"ZENITH INTENSITIES",
        re.MULTILINE,
    )
    hours = list(range(18, 24)) + list(range(0, 7))
    for match in pattern.finditer(text):
        station, wave, yy, mm, dd, body = match.groups()
        wave_i = int(wave)
        if wave_i != wavelength:
            continue
        year = 1900 + int(yy) if int(yy) < 100 else int(yy)
        tokens = body.split()
        # Prefer trailing MEAN/DEV if present; otherwise collect hour fields.
        # Format mixes blanks for missing hours; recover by fixed-width when possible.
        # Fallback: take numeric tokens before MEAN-like values.
        nums: list[float] = []
        for tok in tokens:
            try:
                nums.append(float(tok))
            except ValueError:
                break
        if len(nums) < 2:
            continue
        # Last two numbers are often MEAN and DEV when all hours present; when
        # sparse, still record available intensities without inventing hours.
        intensities = nums[:-2] if len(nums) > 2 else nums
        for hour, value in zip(hours, intensities, strict=False):
            if value <= 0:
                continue
            rows.append(
                HourlyOI(
                    station=station,
                    wavelength_air_angstrom=wave_i,
                    year=year,
                    month=int(mm),
                    day=int(dd),
                    local_hour=hour,
                    intensity_rayleigh=float(value),
                )
            )
    return rows


def assess_abastumani_equivalence(path_5577: Path, path_6300: Path | None = None) -> dict[str, Any]:
    """Apply the oxygen observable-equivalence gate to Abastumani IQSY products."""
    digest_5577 = sha256_file(path_5577)
    rows_5577 = parse_iqsy_hourly(path_5577, wavelength=5577)
    rows_6300 = parse_iqsy_hourly(path_6300, wavelength=6300) if path_6300 and path_6300.is_file() else []
    sample = rows_5577[:5]
    return {
        "schema": "independent-oi-equivalence-v1",
        "dataset": "noaa-ncei-iqsy-abastumani",
        "site": ABASTUMANI,
        "files": {
            "abas5577.int": {
                "sha256": digest_5577,
                "url": ABASTUMANI["source_base"] + "abas5577.int",
                "n_hourly_samples_parsed": len(rows_5577),
            },
            "abas6300.int": {
                "sha256": sha256_file(path_6300) if path_6300 and path_6300.is_file() else None,
                "url": ABASTUMANI["source_base"] + "abas6300.int",
                "n_hourly_samples_parsed": len(rows_6300),
            },
        },
        "observable": "zenith OI line intensity in Rayleighs (hourly local time)",
        "unit_conversion_demo": [
            {
                **asdict(row),
                "photons_cm2_ns_sr": photons_per_ns_sr_from_rayleigh(row.intensity_rayleigh),
            }
            for row in sample
        ],
        "gates": {
            "independence_from_palace_training": (
                "PASS_LIMITED — Abastumani photometry is outside Paranal X-shooter/UVES "
                "training lineage; not used in NSB oxygen development"
            ),
            "timestamps": "PARTIAL — local-hour bins; exact UTC midpoints and exposure windows unresolved",
            "site_geometry": "PASS — published Abastumani coordinates; zenith-only",
            "calibration_provenance": (
                "FAIL — IQSY file states Rayleighs but does not certify absolute "
                "photometer calibration uncertainty or filter transmission"
            ),
            "radiometric_units": "PASS — Rayleighs with documented 4π surface-brightness conversion",
            "absolute_uncertainty": "FAIL — no certified absolute sigma in file",
            "spectral_response": (
                "FAIL — passband/LSF of the historical photometer not provided in the IQSY product"
            ),
            "atmospheric_level": (
                "CONDITIONAL — ground photometer of upper-atmosphere emission; not a TOA spectrum"
            ),
            "model_domain": (
                "FAIL_FOR_DEFAULT — 1964 IQSY epoch; F10.7 and PALACE training-range membership "
                "not established here; Paranal climatology transfer to Georgia unvalidated"
            ),
        },
        "admission": "REJECTED_FOR_ABSOLUTE_DEFAULT_VALIDATION",
        "usable_for": [
            "methodology demonstration of Rayleigh↔NSB unit conversion",
            "order-of-magnitude historical OI intensity context",
            "documentation of what absolute photometer admission still requires",
        ],
        "not_usable_for": [
            "independent absolute H2 admission of PALACE Oxygen",
            "changing NSB Automatic/default Airglow",
            "CTAO camera-rate validation",
        ],
        "carbondale_status": (
            "No NOAA IQSY/IGY product unambiguously identified as SIU Carbondale in this "
            "investigation; Camden (CAMD) IGY files are a different station and are not "
            "substituted."
        ),
    }
