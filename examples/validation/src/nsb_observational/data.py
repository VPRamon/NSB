# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Verified inputs and independently implemented Patat measurement operator."""

from __future__ import annotations

import hashlib
import json
import tarfile
import tempfile
from datetime import date, timedelta
from pathlib import Path
from urllib.request import urlopen

import numpy as np
from astropy.io import fits
from astropy.time import Time

PATAT_SHA = "1f615fccb6dd464ab95b91f34adb4ba8ba3f4beebe97559204483c1da2c9965c"
PATAT_URLS = (
    "https://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar",
    "https://web.archive.org/web/20160329190127id_/http://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar",
)
SOLAR_SHA = "7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96"
SOLAR_SLICE_SHA = "cd44cf6f2e162b763cfe3a216402f4a18d6b2ca88d05907496c2c2c738d225dc"
SOLAR_URL = (
    "https://www.ngdc.noaa.gov/stp/space-weather/solar-data/solar-features/"
    "solar-radio/noontime-flux/penticton/penticton_observed/listings/"
    "listing_drao_noontime-flux-observed_daily.txt"
)
CALIBRATION_NIGHTS = frozenset((51643, 51725, 51761, 51785, 52847))
# Exact SI constants expressed in cgs. 1 nm=10 A; 1 ns=1e-9 s.
HC_ERG_CM = 6.62607015e-27 * 2.99792458e10
ARCSEC2_PER_SR = (180 * 3600 / np.pi) ** 2
UNIT = "photons cm^-2 ns^-1 sr^-1"


def sha256(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def acquire(path, expected, urls=(), *, offline=False):
    """Never replace an invalid cache; only atomically publish verified bytes."""
    path = Path(path)
    if path.is_symlink():
        raise ValueError("Input cache cannot be a symlink")
    if path.exists():
        if sha256(path) != expected:
            raise ValueError(f"Checksum mismatch: {path}; quarantine it explicitly")
        return path
    if offline:
        raise FileNotFoundError(f"Offline input unavailable: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    failures = []
    for url in urls:
        print(f"Acquiring {url}", flush=True)
        try:
            with tempfile.NamedTemporaryFile(dir=path.parent) as out:
                with urlopen(url, timeout=45) as response:
                    while chunk := response.read(1024 * 1024):
                        out.write(chunk)
                        if out.tell() > 100_000_000:
                            raise ValueError("Input exceeds download size limit")
                out.flush()
                if sha256(out.name) != expected:
                    raise ValueError("Complete downloaded SHA-256 mismatch")
                # Hard link has exclusive-create semantics, so concurrent runs cannot overwrite.
                import os

                os.link(out.name, path)
            return path
        except Exception as error:
            failures.append(f"{url}: {type(error).__name__}: {error}")
    raise RuntimeError(
        "No verified input available; no substitute dataset used.\n" + "\n".join(failures)
    )


def extract(archive, destination, *, expected=PATAT_SHA):
    """Read regular flat FITS only; validate every TAR member before writing."""
    archive, destination = Path(archive), Path(destination)
    if sha256(archive) != expected:
        raise ValueError("Unverified archive")
    if any(p.is_symlink() for p in (destination, *destination.parents)):
        raise ValueError("Symlink in extraction destination")
    content = {}
    names = set()
    with tarfile.open(archive) as tar:
        for item in tar:
            name = Path(item.name)
            if (
                not item.isfile()
                or len(name.parts) != 1
                or name.is_absolute()
                or name.name in (".", "..")
                or item.name != name.name
                or item.name in names
                or item.size > 5_000_000
            ):
                raise ValueError(f"Unsafe/duplicate archive member: {item.name}")
            names.add(item.name)
            if item.name.startswith("fxSKY") and item.name.endswith(".fits"):
                content[item.name] = tar.extractfile(item).read()
    if not content:
        raise ValueError("No FITS in archive")
    inventory = {name: hashlib.sha256(data).hexdigest() for name, data in content.items()}
    if destination.exists():
        if {p.name for p in destination.iterdir()} != set(inventory):
            raise ValueError("Extracted inventory mismatch")
        for name, expected_digest in inventory.items():
            p = destination / name
            if p.is_symlink() or not p.is_file() or sha256(p) != expected_digest:
                raise ValueError(f"Extracted FITS changed: {name}")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=destination.parent) as tmp:
            for name, data in content.items():
                (Path(tmp) / name).write_bytes(data)
            Path(tmp).rename(destination)
    return destination, inventory


def linear_integral(x, y, lower, upper):
    """Trapezoid with interpolated boundaries; never extrapolate."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    if (
        x.ndim != 1
        or x.size < 2
        or y.shape != x.shape
        or not np.isfinite(x).all()
        or not np.isfinite(y).all()
        or np.any(np.diff(x) <= 0)
        or not x[0] <= lower < upper <= x[-1]
    ):
        raise ValueError("Finite increasing full-coverage spectral samples required")
    nodes = np.r_[lower, x[(x > lower) & (x < upper)], upper]
    return float(np.trapezoid(np.interp(nodes, x, y), nodes))


def measured_radiance(wavelength_angstrom, energy, lower=365.0, upper=650.0):
    """Exact integral of lambda times piecewise-linear I_E, retaining noisy negatives.

    erg/s/cm2/A/arcsec2 -> photons/cm2/ns/sr. On each clipped interval,
    Simpson integration is exact for the quadratic product lambda*I_E.
    This differs from trapezoid integration of pre-converted photon samples.
    """
    wave = np.asarray(wavelength_angstrom, float)
    flux = np.asarray(energy, float)
    linear_integral(wave, flux, lower * 10, upper * 10)  # validate
    nodes = np.r_[lower * 10, wave[(wave > lower * 10) & (wave < upper * 10)], upper * 10]
    a, b = nodes[:-1], nodes[1:]
    mid = (a + b) / 2
    area = np.sum(
        (b - a)
        / 6
        * (
            a * np.interp(a, wave, flux)
            + 4 * mid * np.interp(mid, wave, flux)
            + b * np.interp(b, wave, flux)
        )
    )
    return float(area * 1e-8 / HC_ERG_CM * ARCSEC2_PER_SR * 1e-9)


def photon_density(wave_angstrom, energy):
    """Per-nm photon density; diagnostic only, broadband uses exact operator above."""
    return np.asarray(energy) * np.asarray(wave_angstrom) * 1e-8 / HC_ERG_CM * ARCSEC2_PER_SR * 1e-8


def exclusion(row):
    """Frozen sequential selection, including DARKFLG; boundaries are intentional."""
    cuts = (
        (row["dark"] != 0, "dark_flag"),
        (row["grism"] != "300V", "grism"),
        (row["filter"] != "FREE", "filter"),
        (row["sun_alt"] >= -18, "twilight"),
        (row["moon_alt"] >= -5, "moon"),
        (not 1 <= row["airmass"] <= 1.5, "airmass"),
        (abs(row["gal_lat"]) < 20, "galactic_latitude"),
        (row["exposure"] < 60, "exposure"),
        (row["wave"][0] > 3650 or row["wave"][-1] < 6500, "coverage"),
    )
    return next((reason for reject, reason in cuts if reject), None)


def load_cohort(directory, *, enforce_registration=True):
    rows, rejected = [], {}
    for path in sorted(Path(directory).glob("*.fits")):
        with fits.open(path) as hdus:
            hdr, flux = hdus[0].header, np.asarray(hdus[0].data, float)
            if hdr["BUNIT"].strip() != "erg/s/cm2/A/arcsec2":
                raise ValueError(f"Unexpected flux unit: {path.name}")
            wave = hdr["CRVAL1"] + (np.arange(hdr["NAXIS1"]) + 1 - hdr["CRPIX1"]) * hdr["CDELT1"]
            fields = {
                "mjd": "MJD-OBS",
                "ra": "RA",
                "dec": "DEC",
                "airmass": "AIRMASS",
                "sun_alt": "ALTSUN",
                "moon_alt": "ALTMOON",
                "gal_lat": "GALAT",
                "header_sfu": "SUNFLUX",
                "exposure": "ORIGEXP",
                "dark": "DARKFLG",
            }
            row = {key: float(hdr[value]) for key, value in fields.items()}
            if not np.isfinite(list(row.values())).all():
                raise ValueError(f"Nonfinite metadata: {path.name}")
            row.update(
                id=path.name,
                night=int(row["mjd"]),
                wave=wave,
                energy=flux,
                grism=hdr["GRISM"].strip(),
                filter=hdr["FILTER"].strip(),
            )
        reason = exclusion(row)
        if reason:
            rejected[row["id"]] = reason
            continue
        row["observed"] = measured_radiance(wave, flux)
        row["split"] = "calibration" if row["night"] in CALIBRATION_NIGHTS else "evaluation"
        if not np.isfinite(row["observed"]) or row["observed"] <= 0:
            raise ValueError("Nonpositive integrated observational radiance")
        rows.append(row)
    identities = "\n".join(f"{r['id']}:{r['night']}:{r['split']}" for r in rows).encode()
    identity_sha = hashlib.sha256(identities).hexdigest()
    if enforce_registration:
        registration = json.loads(Path(__file__).with_name("registration.json").read_text())
        if identity_sha != registration["cohort_identity_sha256"] or len(rows) != 103:
            raise ValueError(
                "Selected observation identities/night partition differ from registration"
            )
    return rows, {
        "selected": len(rows),
        "nights": len({r["night"] for r in rows}),
        "excluded_ids": rejected,
        "identity_sha256": identity_sha,
    }


def solar_windows(path, rows):
    """Only checksum-pinned NOAA full source or frozen factual slice is accepted."""
    path = Path(path)
    digest = sha256(path)
    if digest == SOLAR_SLICE_SHA:
        payload = json.loads(path.read_text())
        if payload["source_sha256"] != SOLAR_SHA:
            raise ValueError("Solar source mismatch")
        daily = {date.fromisoformat(k): v for k, v in payload["daily"].items()}
    elif digest == SOLAR_SHA:
        daily = {}
        for line in path.read_text().splitlines():
            fields = line.split()
            if len(fields) == 2 and fields[1] != ".":
                daily[date(int(fields[0][:4]), int(fields[0][4:6]), int(fields[0][6:8]))] = float(
                    fields[1]
                )
    else:
        raise ValueError("Unverified solar data; matched scenario unavailable")
    values = {}
    for row in rows:
        center = Time(row["mjd"], format="mjd").to_datetime().date()
        days = [center + timedelta(days=i) for i in range(-13, 14)]
        if any(d not in daily for d in days):
            raise ValueError("Incomplete centered 27-day solar window")
        sample = np.array([daily[d] for d in days], float)
        if not np.isfinite(sample).all() or np.any(sample <= 0):
            raise ValueError("Invalid solar input")
        values[row["id"]] = float(np.mean(sample))
    return values
