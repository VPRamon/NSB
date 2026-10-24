"""Patat (2008) FORS1 absolute night-sky spectral library."""

from __future__ import annotations

import hashlib
import tarfile
from dataclasses import asdict, dataclass
from pathlib import Path
from urllib.request import urlopen

import numpy as np
from astropy.io import fits
from numpy.typing import NDArray

ARCHIVE_FALLBACK = (
    "https://web.archive.org/web/20160329190127id_/"
    "http://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar"
)
PRIMARY_URL = "https://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar"


@dataclass(frozen=True)
class PatatSpectrum:
    """One flux-calibrated FORS1 night-sky spectrum with geometry metadata."""

    observation_id: str
    filename: str
    mjd_obs: float
    civil_night: int
    ra_deg: float
    dec_deg: float
    airmass: float
    alt_sun_deg: float
    alt_moon_deg: float
    galactic_latitude_deg: float
    ecliptic_latitude_deg: float
    sun_flux_sfu: float
    original_exposure_s: float
    grism: str
    filter_name: str
    wavelength_angstrom: NDArray[np.float64]
    energy_radiance: NDArray[np.float64]

    def to_dict(self) -> dict:
        payload = asdict(self)
        payload["wavelength_angstrom"] = self.wavelength_angstrom.tolist()
        payload["energy_radiance"] = self.energy_radiance.tolist()
        return payload


def checksum(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def acquire(archive: Path, expected_sha256: str, *, allow_download: bool = True) -> dict:
    """Verify or download the frozen Patat TAR; never overwrite existing bytes."""
    if archive.exists():
        digest = checksum(archive)
        if digest != expected_sha256:
            raise ValueError("Existing Patat archive checksum mismatch")
        return {
            "path": str(archive),
            "sha256": digest,
            "bytes": archive.stat().st_size,
            "source": "preexisting",
        }
    if not allow_download:
        raise FileNotFoundError(archive)
    archive.parent.mkdir(parents=True, exist_ok=True)
    last_error: Exception | None = None
    content: bytes | None = None
    used = ""
    for url in (PRIMARY_URL, ARCHIVE_FALLBACK):
        try:
            with urlopen(url, timeout=120) as response:
                content = response.read()
            used = url
            break
        except Exception as exc:
            last_error = exc
    if content is None:
        raise RuntimeError(f"Patat archive download failed: {last_error}")
    digest = hashlib.sha256(content).hexdigest()
    if digest != expected_sha256:
        raise ValueError("Downloaded Patat archive checksum mismatch")
    tmp = archive.with_suffix(archive.suffix + ".partial")
    tmp.write_bytes(content)
    tmp.replace(archive)
    return {
        "path": str(archive),
        "sha256": digest,
        "bytes": len(content),
        "source": used,
    }


def extract_library(archive: Path, destination: Path) -> Path:
    """Verify every extracted FITS against the caller-verified archive on every run.

    Reuse is fail-closed: missing, modified, extra or symlink products are
    rejected. The historical library and selection are never repaired silently.
    """
    with tarfile.open(archive, "r") as handle:
        expected: dict[str, str] = {}
        for member in handle.getmembers():
            if not member.name.startswith("fxSKY") or not member.name.endswith(".fits"):
                continue
            if not member.isfile() or Path(member.name).name != member.name:
                raise ValueError("Unsafe Patat FITS archive member")
            if member.name in expected:
                raise ValueError("Duplicate Patat FITS archive member")
            stream = handle.extractfile(member)
            if stream is None:
                raise ValueError("Missing Patat FITS member content")
            expected[member.name] = hashlib.sha256(stream.read()).hexdigest()
        if not expected:
            raise ValueError("Patat archive contains no FITS products")
        if not destination.exists():
            destination.mkdir(parents=True, exist_ok=False)
            handle.extractall(destination, filter="data")
    if destination.is_symlink():
        raise ValueError("Patat extraction directory must not be a symlink")
    actual = {p.name: p for p in destination.glob("fxSKY*.fits")}
    if set(actual) != set(expected):
        raise ValueError("Extracted Patat FITS inventory differs from verified archive")
    for name, digest in expected.items():
        path = actual[name]
        if path.is_symlink() or not path.is_file() or checksum(path) != digest:
            raise ValueError(f"Extracted Patat FITS checksum mismatch: {name}")
    return destination


def _wavelength(header: fits.Header) -> NDArray[np.float64]:
    naxis = int(header["NAXIS1"])
    crval = float(header["CRVAL1"])
    cdelt = float(header["CDELT1"])
    crpix = float(header["CRPIX1"])
    return np.asarray(crval + (np.arange(naxis) + 1.0 - crpix) * cdelt, dtype=np.float64)


def load_spectrum(path: Path) -> PatatSpectrum:
    """Load one fxSKY product with required scientific metadata."""
    with fits.open(path) as hdul:
        header = hdul[0].header
        data = np.asarray(hdul[0].data, dtype=float)
    required = [
        "RA",
        "DEC",
        "MJD-OBS",
        "AIRMASS",
        "ALTSUN",
        "ALTMOON",
        "GALAT",
        "ECLAT",
        "SUNFLUX",
        "ORIGEXP",
        "GRISM",
        "FILTER",
        "DARKFLG",
        "BUNIT",
    ]
    missing = [key for key in required if key not in header]
    if missing:
        raise ValueError(f"{path.name}: missing headers {missing}")
    if str(header["BUNIT"]).strip() != "erg/s/cm2/A/arcsec2":
        raise ValueError(f"{path.name}: unexpected BUNIT {header['BUNIT']!r}")
    return PatatSpectrum(
        observation_id=path.name,
        filename=path.name,
        mjd_obs=float(header["MJD-OBS"]),
        civil_night=int(float(header["MJD-OBS"])),
        ra_deg=float(header["RA"]),
        dec_deg=float(header["DEC"]),
        airmass=float(header["AIRMASS"]),
        alt_sun_deg=float(header["ALTSUN"]),
        alt_moon_deg=float(header["ALTMOON"]),
        galactic_latitude_deg=float(header["GALAT"]),
        ecliptic_latitude_deg=float(header["ECLAT"]),
        sun_flux_sfu=float(header["SUNFLUX"]),
        original_exposure_s=float(header["ORIGEXP"]),
        grism=str(header["GRISM"]).strip(),
        filter_name=str(header["FILTER"]).strip(),
        wavelength_angstrom=_wavelength(header),
        energy_radiance=data,
    )


def select_cohort(
    spectra: list[PatatSpectrum], selection: dict
) -> tuple[list[PatatSpectrum], dict]:
    """Apply the preregistered metadata/geometry cohort; retain exclusion counts."""
    wave_min = 10.0 * float(selection.get("wavelength_min_nm", 365.0))
    wave_max = 10.0 * float(selection.get("wavelength_max_nm", 650.0))
    counts: dict[str, int] = {}
    kept: list[PatatSpectrum] = []
    for spectrum in spectra:
        reason = None
        if spectrum.grism != selection["grism"]:
            reason = "grism"
        elif spectrum.filter_name != selection["filter"]:
            reason = "filter"
        elif spectrum.alt_sun_deg >= selection["sun_altitude_max_deg"]:
            reason = "twilight"
        elif spectrum.alt_moon_deg >= selection["moon_altitude_max_deg"]:
            reason = "moon_not_safely_below_horizon"
        elif not (selection["airmass_min"] <= spectrum.airmass <= selection["airmass_max"]):
            reason = "airmass"
        elif abs(spectrum.galactic_latitude_deg) < selection["galactic_latitude_min_abs_deg"]:
            reason = "crowded_galactic_latitude"
        elif spectrum.original_exposure_s < selection["original_exposure_min_s"]:
            reason = "short_exposure"
        elif selection.get("require_full_band_coverage", True) and (
            spectrum.wavelength_angstrom[0] > wave_min + 1.0
            or spectrum.wavelength_angstrom[-1] < wave_max
        ):
            reason = "incomplete_band_coverage"
        # Dark-time flag from Patat: DARKFLG==0 required via moon/sun cuts + library flag.
        if reason is None:
            # Re-read is avoided; dark flag encoded by moonless/dark selection intent.
            kept.append(spectrum)
        else:
            counts[reason] = counts.get(reason, 0) + 1
    # Enforce DARKFLG from files via a second pass on kept using headers already loaded:
    # spectra are loaded only for fxSKY; dark flag checked at load-time selection below.
    return kept, {
        "input": len(spectra),
        "selected": len(kept),
        "nights": len({s.civil_night for s in kept}),
        "exclusions": counts,
    }


def load_selected(library_dir: Path, selection: dict) -> tuple[list[PatatSpectrum], dict]:
    """Load all fxSKY spectra and apply the registered selection, including DARKFLG."""
    paths = sorted(library_dir.glob("fxSKY*.fits"))
    if not paths:
        # tar may extract into cwd-like layout
        paths = sorted(library_dir.rglob("fxSKY*.fits"))
    if not paths:
        raise FileNotFoundError(f"No fxSKY spectra under {library_dir}")
    loaded: list[PatatSpectrum] = []
    dark_rejected = 0
    for path in paths:
        with fits.open(path) as hdul:
            dark = int(hdul[0].header.get("DARKFLG", -1))
        if dark != int(selection["dark_flag"]):
            dark_rejected += 1
            continue
        loaded.append(load_spectrum(path))
    selected, summary = select_cohort(loaded, selection)
    summary["dark_flag_rejected"] = dark_rejected
    summary["loaded_dark_matching"] = len(loaded)
    return selected, summary
