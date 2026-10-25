#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Matched NSB↔nsb2 Starlight benchmark for issue #298 (Outcome B).

Level 1: synthetic numerical-operator cases with independently calculable
expectations (no solar-colour proxy; no production-map retuning).

Level 2: conditional TOA 300–650 nm sky-direction comparisons between NSB's
bundled full-population integrated map and nsb2's Pickles-based Gaia emitters.
Source-ID intersection against NSB aggregates is unsupported; the experimental
Gaia XP 360–570 nm map asset is not redistributed in-repo.

See docs/validation/matched-starlight-nsb2-v1.md.
"""
from __future__ import annotations

import argparse
from datetime import timezone
import hashlib
import json
from pathlib import Path
import platform
import subprocess

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
ARTIFACT = ROOT / "docs/validation/artifacts/matched-starlight-nsb2-v1.json"
SYNTHETIC_MAP = ROOT / "crates/nsb/tests/data/starlight-transport/uniform.json"
PROTOCOL = "matched-starlight-nsb2-v1"
UNIT = "photons cm^-2 ns^-1 sr^-1"
BAND_NM = (300.0, 650.0)
NSB2_COMMIT = "bc9320db03fbff69997ce366c6e76a37339c5d00"
XP_CANDIDATE_SHA = "fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f"
GAIA_CATALOG_URL = "https://zenodo.org/records/15396676/files/gaiadr3.npy"
GAIA_MAP_URL = "https://zenodo.org/records/15396676/files/gaia_mag15plus.npy"
# Frozen before inspecting residuals (ICRS degrees).
SKY_DIRECTIONS = (
    ("galactic_north_pole", 192.85948, 27.12825),
    ("galactic_south_pole", 12.85948, -27.12825),
    ("sparse_high_lat", 180.0, 60.0),
    ("dense_plane", 280.0, -5.0),
    ("anticenter", 86.4, 28.0),
    ("galactic_center", 266.41683, -29.00781),
)
SITE = {"longitude": -70.40498688, "latitude": -24.6275, "height": 2635.0}
MJD = 52075.0  # fixed TOA evaluation epoch; atmosphere excluded


COMPARABILITY_MATRIX = [
    {
        "comparison": "Level-1 synthetic StarlightMap band integrals vs analytic reference",
        "status": "equivalent",
        "quantity": "Bin-integrated TOA photon radiance",
        "units": UNIT,
        "population": "synthetic fixture sources",
        "spectral_domain_nm": [360, 450],
        "atmosphere": "excluded (TOA)",
        "notes": (
            "NSB StarlightMap vs independently calculated analytic expectations. "
            "Not by itself an NSB↔nsb2 cross-implementation equivalence test."
        ),
    },
    {
        "comparison": "Level-1 imposed constant spectrum through NSB StarlightMap and nsb2 SpectralGrid",
        "status": "equivalent",
        "quantity": "Band-integrated TOA photon radiance",
        "units": UNIT,
        "population": "synthetic single-spectrum radiance",
        "spectral_domain_nm": [360, 450],
        "atmosphere": "excluded (TOA)",
        "notes": (
            "Same constant photon spectral radiance density evaluated via NSB's "
            "StarlightMap.integrate_band and nsb2's SpectralGrid/Bandpass/"
            "integrate_wavelength path, each checked against the analytic integral."
        ),
    },
    {
        "comparison": "NSB bundled 300–650 vs nsb2 faint G>15 (same sky directions)",
        "status": "conditional",
        "quantity": "TOA band-integrated photon radiance",
        "units": UNIT,
        "population": "NSB full admitted Gaia+supplement vs nsb2 G>15 map only",
        "spectral_domain_nm": list(BAND_NM),
        "atmosphere": "excluded (TOA)",
        "notes": (
            "Different populations and spectral models (XP/photometric/CK04 vs Pickles). "
            "Matches the notebook's nsb2 emitter choice without the forbidden solar proxy."
        ),
    },
    {
        "comparison": "NSB bundled 300–650 vs nsb2 faint+XHIP",
        "status": "conditional",
        "quantity": "TOA band-integrated photon radiance",
        "units": UNIT,
        "population": "nsb2 G>15 map + XHIP; bright G<15 catalog optional (BallTree cost)",
        "spectral_domain_nm": list(BAND_NM),
        "atmosphere": "excluded (TOA)",
        "notes": (
            "Default Level-2 includes faint+XHIP. Full G<15 BallTree over ~3.7e7 stars is "
            "opt-in via --include-bright-catalog; population counts are always audited."
        ),
    },
    {
        "comparison": "Source-ID matched XP vs Pickles spectra",
        "status": "unsupported",
        "quantity": "Per-source or XP-bin radiance",
        "units": UNIT,
        "population": "Gaia DR3 source_id intersection",
        "spectral_domain_nm": [360, 570],
        "atmosphere": "excluded",
        "notes": (
            "NSB maps expose pixel aggregates, not recoverable source IDs. "
            f"XP candidate {XP_CANDIDATE_SHA} is not bundled in-repo."
        ),
    },
    {
        "comparison": "Solar-colour proxy narrowing of NSB 300–650",
        "status": "unsupported",
        "quantity": "Proxy-scaled band radiance",
        "units": UNIT,
        "population": "n/a",
        "spectral_domain_nm": [365, 650],
        "atmosphere": "n/a",
        "notes": "Forbidden by #298; not a measured stellar spectral product.",
    },
]


# ---------------------------------------------------------------------------
# Level 1 — independent operators
# ---------------------------------------------------------------------------

def healpix_pixel_steradian(nside: int) -> float:
    """Exact HEALPix pixel solid angle Ω = 4π / N_pix."""
    if nside < 1:
        raise ValueError("nside must be positive")
    npix = 12 * nside * nside
    return 4.0 * np.pi / npix


def integrate_photon_density(wave_nm, density_per_nm, lower, upper) -> float:
    wave = np.asarray(wave_nm, float)
    dens = np.asarray(density_per_nm, float)
    if wave.shape != dens.shape or wave.ndim != 1 or wave.size < 2:
        raise ValueError("Invalid spectrum")
    if np.any(np.diff(wave) <= 0):
        raise ValueError("Wavelengths must increase")
    lo, hi = float(lower), float(upper)
    if hi <= lo:
        raise ValueError("Empty band")
    if wave[0] > lo or wave[-1] < hi:
        raise ValueError("Incomplete coverage")
    mask = (wave >= lo) & (wave <= hi)
    if int(mask.sum()) < 2:
        raise ValueError("Insufficient in-band samples")
    return float(np.trapezoid(dens[mask], wave[mask]))


def energy_to_photon_density(wave_nm, energy_per_nm):
    """erg s^-1 cm^-2 nm^-1 → photons s^-1 cm^-2 nm^-1 (per-beam, no sr)."""
    h = 6.62607015e-27
    c = 2.99792458e10
    wave = np.asarray(wave_nm, float)
    energy = np.asarray(energy_per_nm, float)
    return energy * wave * 1e-7 / (h * c)  # nm→cm in wave*1e-7


def sha256_file(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def level1_synthetic_nsb_cases() -> list[dict]:
    """Checksum-pinned synthetic StarlightMap integrals vs analytic sums."""
    import nsb

    sha = sha256_file(SYNTHETIC_MAP)
    mmap = nsb.StarlightMap.load(str(SYNTHETIC_MAP), sha)
    edges = list(mmap.bin_edges_nm)
    # Fixture: every pixel radiance [0.003, 0.006, 0.009] for bins 360-390,390-420,420-450.
    expected_bins = [0.003, 0.006, 0.009]
    cases = []
    for i, (lo, hi, expected) in enumerate(zip(edges[:-1], edges[1:], expected_bins)):
        got, _sigma = mmap.integrate_band(0.0, 0.0, float(lo), float(hi))
        cases.append({
            "case_id": f"L1_synth_bin_{i}_{int(lo)}_{int(hi)}",
            "classification": "equivalent",
            "level": 1,
            "population": "synthetic-uniform-transport-v1",
            "n_sources_declared": 12,
            "wavelength_bounds_nm": [float(lo), float(hi)],
            "observable": "TOA bin-integrated photon radiance",
            "nsb": float(got),
            "reference": float(expected),
            "signed_difference": float(got) - float(expected),
            "absolute_difference": abs(float(got) - float(expected)),
            "relative_difference": None if expected == 0 else abs(float(got) - float(expected)) / abs(expected),
            "units": UNIT,
            "geometry": "HEALPix nside=1 ring galactic; pixel containing (l,b)=(0,0)",
            "provenance": {"synthetic_map_sha256": sha, "path": str(SYNTHETIC_MAP.relative_to(ROOT))},
            "limitations": ["Synthetic fixture only; not a real stellar population."],
        })
    full, _ = mmap.integrate_band(0.0, 0.0, float(edges[0]), float(edges[-1]))
    expected_full = sum(expected_bins)
    cases.append({
        "case_id": "L1_synth_full_native_band",
        "classification": "equivalent",
        "level": 1,
        "population": "synthetic-uniform-transport-v1",
        "n_sources_declared": 12,
        "wavelength_bounds_nm": [float(edges[0]), float(edges[-1])],
        "observable": "TOA native-band photon radiance",
        "nsb": float(full),
        "reference": float(expected_full),
        "signed_difference": float(full) - expected_full,
        "absolute_difference": abs(float(full) - expected_full),
        "relative_difference": abs(float(full) - expected_full) / expected_full,
        "units": UNIT,
        "geometry": "HEALPix nside=1 ring galactic; pixel containing (l,b)=(0,0)",
        "provenance": {"synthetic_map_sha256": sha},
        "limitations": ["Synthetic fixture only."],
    })
    # Partial bin must fail closed.
    try:
        mmap.integrate_band(0.0, 0.0, 360.0, 380.0)
        partial_ok = False
        partial_error = "accepted partial bin"
    except Exception as error:  # noqa: BLE001 — intentional closed-failure probe
        partial_ok = True
        partial_error = str(error)
    cases.append({
        "case_id": "L1_synth_partial_bin_rejected",
        "classification": "equivalent",
        "level": 1,
        "population": "synthetic-uniform-transport-v1",
        "n_sources_declared": 12,
        "wavelength_bounds_nm": [360.0, 380.0],
        "observable": "unsupported partial-bin integral",
        "nsb": None,
        "reference": "reject",
        "signed_difference": None,
        "absolute_difference": None,
        "relative_difference": None,
        "units": UNIT,
        "geometry": "n/a",
        "provenance": {"rejected": partial_ok, "message": partial_error},
        "limitations": [],
        "passed": partial_ok,
    })
    return cases


def level1_analytic_operator_cases() -> list[dict]:
    """Pure-Python radiometry / HEALPix cases (no NSB/nsb2 runtime required)."""
    cases = []
    # Constant spectrum.
    wave = np.linspace(300.0, 650.0, 701)
    dens = np.full_like(wave, 2.0e-4)
    got = integrate_photon_density(wave, dens, 300.0, 650.0)
    expected = 2.0e-4 * 350.0
    cases.append({
        "case_id": "L1_analytic_constant_spectrum",
        "classification": "equivalent",
        "level": 1,
        "nsb": got,
        "reference": expected,
        "signed_difference": got - expected,
        "absolute_difference": abs(got - expected),
        "relative_difference": abs(got - expected) / expected,
        "units": "photons s^-1 cm^-2 sr^-1 (density integral; multiply by 1e-9 for ns)",
        "wavelength_bounds_nm": [300.0, 650.0],
        "observable": "analytic density integral",
        "population": "synthetic",
        "n_sources_declared": 0,
        "geometry": "n/a",
        "provenance": {"method": "trapezoid"},
        "limitations": [],
    })
    # Linear spectrum over a sub-window.
    dens_lin = 1.0e-4 + 1.0e-6 * (wave - 300.0)
    lo, hi = 400.0, 500.0
    got = integrate_photon_density(wave, dens_lin, lo, hi)
    # ∫(a + b(λ-300)) dλ from 400 to 500.
    a, b = 1.0e-4, 1.0e-6
    analytic = (a - 300 * b) * (hi - lo) + 0.5 * b * (hi**2 - lo**2)
    cases.append({
        "case_id": "L1_analytic_linear_spectrum",
        "classification": "equivalent",
        "level": 1,
        "nsb": got,
        "reference": analytic,
        "signed_difference": got - analytic,
        "absolute_difference": abs(got - analytic),
        "relative_difference": abs(got - analytic) / abs(analytic),
        "units": "photons s^-1 cm^-2 sr^-1",
        "wavelength_bounds_nm": [lo, hi],
        "observable": "analytic density integral",
        "population": "synthetic",
        "n_sources_declared": 0,
        "geometry": "n/a",
        "provenance": {"method": "trapezoid"},
        "limitations": [],
    })
    # HEALPix solid angle.
    omega = healpix_pixel_steradian(128)
    expected_omega = 4.0 * np.pi / (12 * 128 * 128)
    cases.append({
        "case_id": "L1_healpix_omega_nside128",
        "classification": "equivalent",
        "level": 1,
        "nsb": omega,
        "reference": expected_omega,
        "signed_difference": omega - expected_omega,
        "absolute_difference": abs(omega - expected_omega),
        "relative_difference": abs(omega - expected_omega) / expected_omega,
        "units": "sr",
        "wavelength_bounds_nm": None,
        "observable": "pixel solid angle",
        "population": "n/a",
        "n_sources_declared": 0,
        "geometry": "HEALPix nside=128",
        "provenance": {},
        "limitations": [],
    })
    return cases


# ---------------------------------------------------------------------------
# Population audit
# ---------------------------------------------------------------------------

def population_audit(offline: bool) -> dict:
    """Document populations; never assign missing sets to zero radiance."""
    audit = {
        "nsb_production_integrated_300_650": {
            "product": "bundled starlight-runtime-v1 compact map",
            "band_nm": [300, 650],
            "healpix": {"nside": 128, "ordering": "ring", "frame": "galactic"},
            "domain": "top-of-atmosphere",
            "admitted_sources": 1_518_801_751,
            "excluded_sources": 292_908_075,
            "observed_gaia_rows": 1_811_709_826,
            "bright_star_supplement_admitted_records": 55,
            "spectral_model": (
                "Gaia XP / photometric inference for Gaia population; "
                "Hipparcos/XHIP-selected CK04 supplement"
            ),
            "source_ids_in_runtime_map": False,
            "sub_band_reconstruction": False,
            "evidence": "docs/nsb_components/starlight/production-runs/combined-300-650-validation.json",
        },
        "nsb_experimental_xp_360_570": {
            "product_sha256": XP_CANDIDATE_SHA,
            "band_nm": [360, 570],
            "native_bins_nm": [360, 390, 420, 450, 480, 510, 540, 570],
            "healpix": {"nside": 128, "ordering": "ring", "frame": "galactic"},
            "maturity": "experimental",
            "population": "XP-only; not population-complete",
            "historical_raw_xp_records": 219_197_643,
            "canonical_xp_rows": 219_197_642,
            "bundled_in_repository": False,
            "available_locally_this_run": False,
            "status": "unsupported_without_external_asset",
        },
        "nsb2": {
            "commit": NSB2_COMMIT,
            "gaia_split_magnitude": 15,
            "bright_catalog": {
                "url": GAIA_CATALOG_URL,
                "selection": "G < 15 individually resolved",
                "n_sources": None,
                "has_source_id": True,
                "spectral_model": "Pickles TRDS Atlas 1998 + Gaia G/BP/RP photometry",
            },
            "faint_map": {
                "url": GAIA_MAP_URL,
                "selection": "G > 15 pre-binned HEALPix",
                "healpix": {"nside": 1024, "ordering": "nested", "frame": "icrs"},
                "source_ids": False,
                "spectral_model": "Pickles templates from mean cell colour",
            },
            "xhip_supplement": {
                "n_sources": 88,
                "spectral_model": "Pickles from Johnson V/B",
                "note": "Stars too bright for reliable Gaia DR3 photometry",
            },
        },
        "intersection": {
            "exact_source_id_match_against_nsb_runtime_maps": "unsupported",
            "reason": (
                "NSB runtime products store HEALPix aggregates; per-source Gaia IDs "
                "are not recoverable from the bundled map or from the absent XP JSON."
            ),
            "missing_represented_as": "unsupported / unavailable — never silent zero",
        },
    }
    if offline:
        return audit
    try:
        from astropy.utils.data import download_file

        bright_path = Path(download_file(GAIA_CATALOG_URL, cache=True, show_progress=False))
        faint_path = Path(download_file(GAIA_MAP_URL, cache=True, show_progress=False))
        bright = np.load(bright_path)
        faint = np.load(faint_path)
        audit["nsb2"]["bright_catalog"].update({
            "n_sources": int(len(bright)),
            "unique_source_ids": int(len(np.unique(bright["source_id"]))),
            "g_min": float(np.nanmin(bright["phot_g_mean_mag"])),
            "g_max": float(np.nanmax(bright["phot_g_mean_mag"])),
            "sha256": sha256_file(bright_path),
        })
        audit["nsb2"]["faint_map"].update({
            "shape": list(faint.shape),
            "finite_cells": int(np.isfinite(faint[0]).sum()),
            "sha256": sha256_file(faint_path),
        })
    except Exception as error:  # noqa: BLE001
        audit["nsb2"]["asset_load_error"] = str(error)
    return audit


# ---------------------------------------------------------------------------
# Level 2 — conditional TOA sky directions
# ---------------------------------------------------------------------------

def nsb_toa_starlight(ra_deg: float, dec_deg: float) -> float:
    import nsb
    import siderust
    from astropy.time import Time

    cfg = nsb.NsbModelConfig.generic_clear_sky()
    ev = nsb.NsbEvaluator(cfg)
    obs = siderust.Observer(
        lon_deg=SITE["longitude"], lat_deg=SITE["latitude"], height_m=SITE["height"]
    )
    t = Time(MJD, format="mjd", scale="utc").to_datetime(timezone=timezone.utc)
    query = nsb.PointQuery(
        obs,
        t,
        siderust.Direction(ra_deg=ra_deg, dec_deg=dec_deg),
        components=nsb.ComponentMask.STARLIGHT,
    )
    result = ev.evaluate(query)
    return float(result.components[0].integrated_photons_cm2_ns_sr)


class UnavailableSpectralTemplate(ValueError):
    """A selected source has no finite Pickles template rate (not a zero flux)."""


def reduce_pickles_template_rates(rates_nc) -> float:
    """Sum per-source nanmedians of finite Pickles template rates.

    Distinguishes:
    - empty selection (N=0) → legitimate 0.0
    - selected source with no finite templates → UnavailableSpectralTemplate
    - selected source with mixed finite/non-finite templates → nanmedian of finite
    - selected source with valid numerical zero → contributes 0.0

    Never silently drops an invalid selected source while summing the rest.
    """
    rates = np.asarray(rates_nc, float)
    if rates.size == 0:
        return 0.0
    rates = np.atleast_2d(rates)
    per_source = []
    for index, row in enumerate(rates):
        ok = row[np.isfinite(row)]
        if ok.size == 0:
            raise UnavailableSpectralTemplate(
                f"selected source index {index} has no finite Pickles template rate"
            )
        per_source.append(float(np.median(ok)))
    return float(np.sum(per_source))


def _nsb2_band_integral(field, lower: float, upper: float) -> float:
    """Flat top-hat band integral with fail-closed Pickles template reduction.

    Empty catalog fields (no star in the search cone) contribute exactly zero —
    not an imputed population. Selected sources with no finite templates reject
    the evaluation rather than vanishing into a partial sum.
    """
    import astropy.units as u
    from nsb2.core.spectral import Bandpass

    n_sources = int(np.asarray(field.spectral_data).shape[0])
    if n_sources == 0:
        return 0.0
    # Same four-node flat bandpass shape used by the observational notebook.
    band = Bandpass(np.array([lower, 400.0, 550.0, upper]) * u.nm, np.ones(4))
    rate = field.resolve_spectra(band).integrate()
    shaped = np.asarray(
        rate.to_value(1 / u.s / u.cm**2 / u.sr, equivalencies=u.dimensionless_angles()),
        float,
    )
    shaped = np.atleast_2d(shaped)
    if shaped.shape[0] != n_sources:
        raise ValueError(
            f"nsb2 rate rows {shaped.shape[0]} != selected sources {n_sources}"
        )
    return reduce_pickles_template_rates(shaped) * 1e-9


def nsb2_search_radius_rad() -> float:
    """Catalog cone matched to NSB nside=128 HEALPix resolution."""
    import healpy as hp

    return float(hp.nside2resol(128))


def nsb2_toa_components(ra_deg: float, dec_deg: float, sources: dict) -> dict[str, float]:
    import astropy.units as u
    from astropy.coordinates import AltAz, EarthLocation, SkyCoord
    from astropy.time import Time
    from types import SimpleNamespace

    site = EarthLocation.from_geodetic(
        SITE["longitude"] * u.deg, SITE["latitude"] * u.deg, SITE["height"] * u.m
    )
    frame = AltAz(
        obstime=Time(MJD, format="mjd", scale="utc"),
        location=site,
        pressure=0 * u.hPa,
    )
    direction = SkyCoord(ra=[ra_deg] * u.deg, dec=[dec_deg] * u.deg)
    catalog_radius = np.array([nsb2_search_radius_rad()])
    out = {}
    for name, source in sources.items():
        # HEALPix radiance maps interpolate at the direction (radius unused / zero).
        # Catalog sources need a finite cone; use NSB nside=128 resolution.
        is_catalog = hasattr(source, "balltree") or hasattr(source, "build_balltree")
        radii = catalog_radius if is_catalog else np.zeros(1)
        field, _ = source.query_direct(SimpleNamespace(origin=frame), direction, radii)
        out[name] = _nsb2_band_integral(field, BAND_NM[0], BAND_NM[1])
    out["total"] = float(sum(out.values()))
    return out


def load_nsb2_sources(*, include_bright_catalog: bool) -> dict:
    from nsb2.emitter.stars import (
        from_gaia_dr3_catalog,
        from_gaia_dr3_map,
        from_gaia_suppl_catalog,
    )

    faint = from_gaia_dr3_map()
    suppl = from_gaia_suppl_catalog()
    suppl.build_balltree()
    sources = {"faint": faint, "suppl": suppl}
    if include_bright_catalog:
        # ~3.7e7 stars: BallTree construction is intentionally opt-in.
        bright = from_gaia_dr3_catalog()
        bright.build_balltree()
        sources["bright"] = bright
    return sources


def level2_sky_direction_cases(*, include_bright_catalog: bool = False) -> list[dict]:
    sources = load_nsb2_sources(include_bright_catalog=include_bright_catalog)
    cases = []
    for name, ra, dec in SKY_DIRECTIONS:
        nsb_val = nsb_toa_starlight(ra, dec)
        parts = nsb2_toa_components(ra, dec, sources)
        configs = [
            (
                "nsb2_faint_only",
                parts["faint"],
                "nsb2 G>15 HEALPix map only (notebook emitter)",
            ),
            (
                "nsb2_faint_xhip",
                parts["faint"] + parts["suppl"],
                "nsb2 G>15 map + XHIP supplement (default extended)",
            ),
        ]
        if include_bright_catalog:
            configs.append(
                (
                    "nsb2_faint_bright_xhip",
                    parts["total"],
                    "nsb2 G>15 map + G<15 catalog + XHIP supplement",
                )
            )
        for label, nsb2_val, population in configs:
            signed = nsb2_val - nsb_val
            cases.append({
                "case_id": f"L2_{name}__nsb_vs_{label}",
                "classification": "conditional",
                "level": 2,
                "direction": {"name": name, "ra_deg": ra, "dec_deg": dec, "frame": "icrs"},
                "population": population,
                "n_sources_declared": None,
                "wavelength_bounds_nm": list(BAND_NM),
                "observable": "TOA band-integrated photon radiance",
                "model_nsb": "bundled production Gaia DR3 integrated Starlight 300–650 nm",
                "model_nsb2": label,
                "nsb": nsb_val,
                "nsb2": nsb2_val,
                "nsb2_parts": {k: parts[k] for k in parts if k != "total"},
                "signed_difference": signed,
                "absolute_difference": abs(signed),
                "relative_difference": None if nsb_val == 0 else abs(signed) / abs(nsb_val),
                "units": UNIT,
                "geometry": (
                    "Pointing ICRS direction; NSB Galactic nside=128 RING pixel mean; "
                    "nsb2 ICRS nested map interpolation / catalog search; atmosphere excluded"
                ),
                "provenance": {
                    "nsb2_commit": NSB2_COMMIT,
                    "mjd": MJD,
                    "site": SITE,
                    "pickles_template_reduction": (
                        "nanmedian of finite colour-grid templates "
                        "(nsb2 instrument central of nanmin/nanmedian/nanmax)"
                    ),
                    "include_bright_catalog": include_bright_catalog,
                },
                "limitations": [
                    "Not source-ID matched.",
                    "Spectral models differ (NSB XP/photometric/CK04 vs nsb2 Pickles).",
                    "HEALPix nside/frame/ordering differ.",
                    "Cross-implementation agreement ≠ observational validation.",
                    *(
                        []
                        if include_bright_catalog
                        else [
                            "Bright G<15 catalog omitted from default Level-2 integrals; "
                            "population count is audited separately."
                        ]
                    ),
                ],
            })
    return cases


def git_rev(path: Path) -> str | None:
    try:
        return subprocess.check_output(
            ["git", "-C", str(path), "rev-parse", "HEAD"], text=True
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def package_version(name: str) -> str | None:
    try:
        import importlib.metadata as metadata

        return metadata.version(name)
    except Exception:  # noqa: BLE001
        return None


def _direct_url_commit(dist) -> str | None:
    raw = dist.read_text("direct_url.json")
    if not raw:
        return None
    payload = json.loads(raw)
    vcs = payload.get("vcs_info") or {}
    commit = vcs.get("commit_id")
    return str(commit) if commit else None


def git_toplevel(path: Path) -> Path | None:
    try:
        text = subprocess.check_output(
            ["git", "-C", str(path), "rev-parse", "--show-toplevel"], text=True
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    return Path(text) if text else None


def _looks_like_nsb2_source_tree(root: Path) -> bool:
    """Accept git metadata only when the repository is an nsb2 source checkout.

    Prevents a site-packages install nested inside the NSB repo from reporting
    NSB's HEAD as the nsb2 revision.
    """
    pyproject = root / "pyproject.toml"
    if not pyproject.is_file():
        return False
    try:
        text = pyproject.read_text(encoding="utf-8")
    except OSError:
        return False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("name") and "=" in stripped:
            _, _, value = stripped.partition("=")
            if value.strip().strip("\"'") == "nsb2":
                return True
            break
    return False


def _git_commit_owned_by_nsb2(module_file: Path) -> tuple[str | None, str | None]:
    module_path = module_file.resolve()
    for candidate in (module_path.parent, module_path.parent.parent):
        toplevel = git_toplevel(candidate)
        if toplevel is None:
            continue
        try:
            module_path.relative_to(toplevel)
        except ValueError:
            continue
        if not _looks_like_nsb2_source_tree(toplevel):
            continue
        commit = git_rev(toplevel)
        if commit:
            return commit, f"git:{toplevel}"
    return None, None


def resolve_nsb2_identity() -> dict:
    """Resolve the installed nsb2 revision without network access."""
    import importlib.metadata as metadata
    import nsb2

    identity = {
        "pinned_commit": NSB2_COMMIT,
        "package_version": package_version("nsb2"),
        "module_path": str(Path(nsb2.__file__).resolve()),
        "resolved_commit": None,
        "resolution_source": None,
        "verification": "unverifiable",
    }
    commit, source = _git_commit_owned_by_nsb2(Path(nsb2.__file__))
    if commit:
        identity["resolved_commit"] = commit
        identity["resolution_source"] = source
    if identity["resolved_commit"] is None:
        try:
            dist = metadata.distribution("nsb2")
            commit = _direct_url_commit(dist)
            if commit:
                identity["resolved_commit"] = commit
                identity["resolution_source"] = "direct_url.json:vcs_info.commit_id"
        except Exception:  # noqa: BLE001
            pass
    resolved = identity["resolved_commit"]
    if resolved == NSB2_COMMIT:
        identity["verification"] = "matched"
    elif resolved is not None:
        identity["verification"] = "mismatched"
    return identity


def require_nsb2_pin(*, allow_unverified: bool = False) -> dict:
    """Option A: require pinned revision. Option B: allow explicit provenance limit."""
    identity = resolve_nsb2_identity()
    if identity["verification"] == "matched":
        identity["evidence_status"] = "pinned_reference"
        return identity
    if identity["verification"] == "mismatched":
        if not allow_unverified:
            raise RuntimeError(
                "Installed nsb2 commit "
                f"{identity['resolved_commit']} does not match pinned {NSB2_COMMIT}"
            )
        identity["evidence_status"] = "provenance_limited_mismatched"
        return identity
    if not allow_unverified:
        raise RuntimeError(
            "Installed nsb2 revision is unverifiable (no nsb2-owned git checkout or "
            "direct_url vcs_info). Refusing to publish falsely pinned evidence. "
            "Use --allow-unverified-nsb2 only for explicitly provenance-limited runs."
        )
    identity["evidence_status"] = "provenance_limited_unverified"
    return identity


def level1_cross_implementation_constant_spectrum() -> list[dict]:
    """Paired NSB↔nsb2 evaluation of the same imposed constant TOA spectrum.

    Uses NSB StarlightMap.integrate_band and nsb2 SpectralGrid + Bandpass +
    integrate_wavelength on independently constructed representations of the
    same constant photon spectral radiance density. A shared NumPy helper is
    not used as the only reference.
    """
    import tempfile

    import astropy.units as u
    import nsb
    from nsb2.core.dtypes import ResolvedField
    from nsb2.core.spectral import Bandpass, integrate_wavelength
    from astropy.coordinates import SkyCoord

    density_per_nm = 1.0e-4  # photons cm^-2 ns^-1 sr^-1 nm^-1
    lo, hi = 360, 450
    analytic = density_per_nm * (hi - lo)
    # Three equal native bins; StarlightMap edges are integer nm (u16).
    edges = [360, 390, 420, 450]
    bin_radiance = [density_per_nm * 30.0] * 3
    pixels = [
        {"pixel": i, "sources": 1, "radiance": bin_radiance} for i in range(12)
    ]
    document = {
        "schema": "nsb-starlight-spectral-map-v2",
        "product_id": "synthetic-constant-360-450-cross-v1",
        "band_nm": [lo, hi],
        "bin_edges_nm": edges,
        "nside": 1,
        "ordering": "ring",
        "coordinate_frame": "galactic",
        "domain": "top-of-atmosphere",
        "units": "photon/(cm2 ns sr)",
        "wavelength_convention": "vacuum",
        "maturity": "experimental",
        "provenance": {
            "source_catalogue": "synthetic",
            "source_license": "AGPL-3.0-only",
            "source_selection": "imposed constant spectrum",
            "generation_method": "matched-starlight benchmark fixture",
            "limitations": ["synthetic cross-implementation fixture"],
        },
        "catalogue_rows": 12,
        "admitted_sources": 12,
        "excluded_sources": {},
        "pixels": pixels,
    }
    raw = (json.dumps(document, separators=(",", ":")) + "\n").encode()
    sha = hashlib.sha256(raw).hexdigest()
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as handle:
        handle.write(raw)
        path = Path(handle.name)
    try:
        mmap = nsb.StarlightMap.load(str(path), sha)
        nsb_value, _ = mmap.integrate_band(0.0, 0.0, float(lo), float(hi))
        nsb_value = float(nsb_value)
    finally:
        path.unlink(missing_ok=True)

    # nsb2 path: constant density in per-second units; solid angle via weight.
    dens_per_s = density_per_nm * 1e9
    wave = np.linspace(float(lo), float(hi), 901) * u.nm
    flx = np.full((wave.size, 1), dens_per_s) * (1 / u.s / u.cm**2 / u.nm)
    from nsb2.core.spectral import SpectralGrid

    grid = SpectralGrid([], wave, flx)
    # UnivariateSpline (k=3) requires >3 nodes; flat top-hat over the band.
    band = Bandpass(
        np.array([float(lo), 390.0, 420.0, float(hi)]) * u.nm, np.ones(4)
    )
    restricted = grid.apply_bandpass(band)
    weights = np.array([[1.0]]) / u.sr
    # (N=1, W, C=1) radiance field for ResolvedField.integrate.
    flx_n = restricted.flx[None, ...]
    resolved = ResolvedField(
        coords=SkyCoord([0] * u.deg, [0] * u.deg, frame="galactic"),
        weights=weights,
        wvl=restricted.wvl,
        flx=flx_n,
        radiance_field=True,
    )
    rate = resolved.integrate()
    nsb2_per_s = float(
        np.asarray(
            rate.to_value(1 / u.s / u.cm**2 / u.sr, equivalencies=u.dimensionless_angles())
        ).reshape(-1)[0]
    )
    nsb2_value = nsb2_per_s * 1e-9
    # Direct nsb2.integrate_wavelength on the same weighted spectrum.
    direct = integrate_wavelength(weights[:, None] * flx_n, restricted.wvl, axis=-2)
    direct_value = float(
        np.asarray(
            direct.to_value(1 / u.s / u.cm**2 / u.sr, equivalencies=u.dimensionless_angles())
        ).reshape(-1)[0]
    ) * 1e-9

    cases = []
    for case_id, left, right, label_a, label_b in (
        ("L1_cross_nsb_vs_analytic", nsb_value, analytic, "nsb", "analytic"),
        ("L1_cross_nsb2_vs_analytic", nsb2_value, analytic, "nsb2", "analytic"),
        ("L1_cross_nsb_vs_nsb2", nsb_value, nsb2_value, "nsb", "nsb2"),
        ("L1_cross_nsb2_integrate_vs_resolved", nsb2_value, direct_value, "nsb2_resolved", "nsb2_integrate_wavelength"),
    ):
        signed = float(left) - float(right)
        cases.append({
            "case_id": case_id,
            "classification": "equivalent",
            "level": 1,
            "population": "synthetic imposed constant spectrum",
            "n_sources_declared": 1,
            "wavelength_bounds_nm": [lo, hi],
            "observable": "TOA band-integrated photon radiance",
            "nsb": nsb_value if "nsb" in label_a or label_a == "nsb" else None,
            "nsb2": nsb2_value,
            "left": float(left),
            "right": float(right),
            "compared_keys": [label_a, label_b],
            "reference": analytic,
            "signed_difference": signed,
            "absolute_difference": abs(signed),
            "relative_difference": None if right == 0 else abs(signed) / abs(right),
            "units": UNIT,
            "geometry": "synthetic isotropic radiance; NSB nside=1 galactic RING",
            "provenance": {
                "density_photons_cm2_ns_sr_nm": density_per_nm,
                "nsb_map_sha256": sha,
                "nsb2_operators": [
                    "ResolvedField.integrate",
                    "integrate_wavelength",
                    "Bandpass",
                ],
            },
            "limitations": [
                "Synthetic imposed spectrum only; not a Gaia population comparison.",
            ],
        })
    return cases


def summarize_cases(cases: list[dict], key_a: str, key_b: str) -> dict | None:
    if not cases:
        return None
    rel = [c["relative_difference"] for c in cases if c.get("relative_difference") is not None]
    absdiff = [c["absolute_difference"] for c in cases if c.get("absolute_difference") is not None]
    return {
        "n_cases": len(cases),
        "mean_absolute_difference": float(np.mean(absdiff)) if absdiff else None,
        "median_relative_difference": float(np.median(rel)) if rel else None,
        "max_relative_difference": float(np.max(rel)) if rel else None,
        "compared_keys": [key_a, key_b],
    }


def build_artifact(
    *,
    skip_level2: bool,
    offline_audit: bool,
    include_bright_catalog: bool,
    allow_unverified_nsb2: bool,
) -> dict:
    # Level-1 cross-implementation and Level-2 both import nsb2 operators.
    nsb2_identity = require_nsb2_pin(allow_unverified=allow_unverified_nsb2)

    level1 = (
        level1_analytic_operator_cases()
        + level1_synthetic_nsb_cases()
        + level1_cross_implementation_constant_spectrum()
    )
    level2: list[dict] = []
    level2_error = None
    if not skip_level2:
        try:
            level2 = level2_sky_direction_cases(
                include_bright_catalog=include_bright_catalog
            )
        except Exception as error:  # noqa: BLE001
            level2_error = str(error)

    equivalent = [c for c in level1 if c.get("classification") == "equivalent"]
    cross = [c for c in equivalent if c["case_id"].startswith("L1_cross_")]
    conditional = [c for c in level2 if c.get("classification") == "conditional"]

    return {
        "schema": PROTOCOL,
        "issue": 298,
        "outcome": "B_restricted_equivalent_benchmark",
        "issue_298_closure": {
            "closes": True,
            "justification": (
                "Acceptance requires a comparison contract, at least one genuinely "
                "equivalent numerical paired test, explicit missing-population "
                "accounting, and distinction from empirical validation. Level-1 "
                "includes an NSB↔nsb2 imposed-spectrum paired test through each "
                "implementation's spectral operators plus analytic checks; Level-2 "
                "remains conditional. Missing populations are not treated as zero."
            ),
        },
        "scientific_question": (
            "When NSB and nsb2 represent stellar sources over a shared wavelength "
            "interval with the same physical observation operator, how closely do "
            "their predictions agree?"
        ),
        "unit": UNIT,
        "band_nm_level2": list(BAND_NM),
        "atmosphere": "excluded_toa_primary",
        "comparability_matrix": COMPARABILITY_MATRIX,
        "population_audit": population_audit(offline=offline_audit),
        "frozen_sky_directions": [
            {"name": n, "ra_deg": ra, "dec_deg": dec} for n, ra, dec in SKY_DIRECTIONS
        ],
        "provenance": {
            "nsb_git": git_rev(ROOT),
            "nsb2_commit_pinned": NSB2_COMMIT,
            "nsb2_identity": nsb2_identity,
            "nsb2_package_version": package_version("nsb2"),
            "nsb_package_version": package_version("nsb"),
            "pickles_template_reduction": (
                "nanmedian over finite Pickles template alternatives "
                "(nsb2 instrument central of nanmin/nanmedian/nanmax); "
                "selected sources with no finite template reject the evaluation"
            ),
            "python": platform.python_version(),
            "platform": platform.platform(),
        },
        "forbidden_operations_not_used": [
            "solar_colour_proxy_on_nsb_300_650",
            "arbitrary_renormalization",
            "treating_missing_population_as_zero",
            "silent_drop_of_invalid_pickles_templates",
        ],
        "level1_cases": level1,
        "level2_cases": level2,
        "level2_execution_error": level2_error,
        "aggregates": {
            "level1_equivalent": summarize_cases(equivalent, "left_or_nsb", "right_or_reference"),
            "level1_cross_implementation": summarize_cases(cross, "left", "right"),
            "level2_conditional_faint_only": summarize_cases(
                [c for c in conditional if c["case_id"].endswith("nsb2_faint_only")],
                "nsb",
                "nsb2",
            ),
            "level2_conditional_faint_xhip": summarize_cases(
                [c for c in conditional if c["case_id"].endswith("nsb2_faint_xhip")],
                "nsb",
                "nsb2",
            ),
            "level2_conditional_faint_bright_xhip": summarize_cases(
                [c for c in conditional if c["case_id"].endswith("nsb2_faint_bright_xhip")],
                "nsb",
                "nsb2",
            ),
        },
        "demonstrated_causes": [
            {
                "cause": "dataset_population_mismatch",
                "evidence": (
                    "NSB admits ~1.52e9 Gaia sources (+55 bright supplements); "
                    "nsb2 faint map has no source IDs and excludes G<15; bright "
                    "catalog has 3.69e7 sources with G<15."
                ),
            },
            {
                "cause": "intentional_scientific_model_difference",
                "evidence": (
                    "NSB uses XP/photometric/CK04 construction; nsb2 uses Pickles "
                    "templates keyed by Gaia colour. Colour grids retain multiple "
                    "finite templates; Level-2 uses nanmedian of finite alternatives "
                    "and rejects selected sources with no finite template."
                ),
            },
            {
                "cause": "geometry_definition_difference",
                "evidence": (
                    "NSB Galactic RING nside=128 vs nsb2 ICRS NESTED nside=1024 faint map."
                ),
            },
            {
                "cause": "unsupported_observable",
                "evidence": (
                    "Source-ID XP↔Pickles and solar-proxy narrowband comparisons rejected."
                ),
            },
        ],
        "limitations": [
            "Cross-implementation agreement is not empirical validation.",
            "Level-2 cases are conditional, not population-equivalent.",
            "XP 360–570 candidate bytes are unavailable in this repository checkout.",
            "No production Starlight coefficients were changed.",
            "Level-1 NSB-vs-analytic cases alone do not establish NSB↔nsb2 equivalence; "
            "the imposed-spectrum cross cases do.",
        ],
        "follow_ups": [
            "Obtain checksum-pinned XP map bytes to enable 360–570 nm spectral comparisons (#304 context).",
            "If source-level matching is required, build a small offline Gaia ID subset with admitted regeneration tooling — do not pretend map pixels identify sources.",
            "Atmospheric matched transport remains #187 / #304; prior ~0.00275% single-scattering reference is numerical only.",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ARTIFACT)
    parser.add_argument("--skip-level2", action="store_true",
                        help="Run Level-1 + audit only (no nsb2 sky directions)")
    parser.add_argument("--offline-audit", action="store_true",
                        help="Do not touch Zenodo/Astropy caches for population counts")
    parser.add_argument(
        "--include-bright-catalog",
        action="store_true",
        help="Also build the nsb2 G<15 BallTree (~3.7e7 stars; slow, opt-in)",
    )
    parser.add_argument(
        "--allow-unverified-nsb2",
        action="store_true",
        help=(
            "Option B: allow Level-1/2 when nsb2 commit cannot be verified; "
            "marks evidence as provenance-limited (default refuses)"
        ),
    )
    args = parser.parse_args()
    artifact = build_artifact(
        skip_level2=args.skip_level2,
        offline_audit=args.offline_audit,
        include_bright_catalog=args.include_bright_catalog,
        allow_unverified_nsb2=args.allow_unverified_nsb2,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(artifact, indent=2, sort_keys=True) + "\n")
    print(f"Wrote {args.output}")
    print("outcome", artifact["outcome"])
    print("nsb2_identity", artifact["provenance"].get("nsb2_identity"))
    if artifact["level2_execution_error"]:
        print("level2_error", artifact["level2_execution_error"])
    for key, summary in artifact["aggregates"].items():
        print(key, summary)


if __name__ == "__main__":
    main()
