#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Frozen spectral-window residual diagnostics for NSB issue #296.

Outcome B: restricted continuum-subtracted Oxygen-window comparisons against
NSB Oxygen-only band integrals. Full total-sky versus Airglow-only spectral MAE
is explicitly unsupported. Protocol, windows and operators are frozen before
residual inspection; see docs/validation/spectral-window-diagnostics-v1.md.
"""
from __future__ import annotations

import argparse
from datetime import UTC, date, timedelta
import hashlib
from io import BytesIO
import json
from pathlib import Path
import tarfile

import numpy as np
from siderust.direction import ICRS

ROOT = Path(__file__).resolve().parents[1]
ARTIFACT = ROOT / "docs/validation/artifacts/spectral-window-diagnostics-v1.json"
PROTOCOL_VERSION = "spectral-window-diagnostics-v1"
SEED = 182
BOOTSTRAP = 2000

# Exact SI constants in cgs, matching the registered notebook.
HC_ERG_CM = 6.62607015e-27 * 2.99792458e10
ARCSEC2_PER_SR = (180 * 3600 / np.pi) ** 2
UNIT = "photons cm^-2 ns^-1 sr^-1"

# PALACE Oxygen vacuum line centres (airglow_palace_oxygen_v1.json).
PALACE_VACUUM_NM = {
    "oi_green": 557.8887,
    "oi_630": 630.2046,
    "oi_636": 636.5536,
    "oi_639": 639.35,
}

# Frozen half-width and sideband geometry (oxygen-and-calibration-v1.md).
HALF_WIDTH_NM = 2.5
SIDEBAND_INNER_NM = 3.0
SIDEBAND_OUTER_NM = 5.0

# Continuum-control bands: descriptive only (non-equivalent components).
CONTINUUM_CONTROL_AIR_NM = {
    "cont_blue_400_430": (400.0, 430.0),
    "cont_mid_450_500": (450.0, 500.0),
}

SITE = {"longitude": -70.4045, "latitude": -24.6272, "height": 2635.0}
PATAT_SHA = "1f615fccb6dd464ab95b91f34adb4ba8ba3f4beebe97559204483c1da2c9965c"
SOLAR_SHA = "7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96"
COHORT_SHA = "3efdd3a0f3af19bfbc7615864f928b33fe22bcdbd6c67568fe317c059d44a1d6"
CALIBRATION_NIGHTS = frozenset((51643, 51725, 51761, 51785, 52847))

# ---------------------------------------------------------------------------
# Wavelength conventions (Morton 2000 / IAU standard optical conversion)
# ---------------------------------------------------------------------------

def refractive_index_air_morton2000(wavelength_vacuum_angstrom: float) -> float:
    """Air refractive index for optical vacuum wavelengths (Morton 2000).

    Valid for dry air at 15 C, 760 Torr. Optical domain ~2000–10000 Angstrom.
    """
    wave = float(wavelength_vacuum_angstrom)
    if not np.isfinite(wave) or wave < 2000.0 or wave > 10000.0:
        raise ValueError(
            f"Morton 2000 conversion unsupported outside 2000–10000 A; got {wave}"
        )
    sigma2 = (1.0e4 / wave) ** 2
    return (
        1.0
        + 0.0000834254
        + 0.02406147 / (130.0 - sigma2)
        + 0.00015998 / (38.9 - sigma2)
    )


def vacuum_to_air_nm(wavelength_vacuum_nm: float) -> float:
    """Convert vacuum nanometres to air nanometres (Morton 2000)."""
    vac_a = float(wavelength_vacuum_nm) * 10.0
    return vac_a / refractive_index_air_morton2000(vac_a) / 10.0


def air_to_vacuum_nm(wavelength_air_nm: float) -> float:
    """Invert Morton 2000 by fixed-point iteration (optical domain)."""
    air = float(wavelength_air_nm)
    if not np.isfinite(air) or air < 200.0 or air > 1000.0:
        raise ValueError(f"Unsupported air wavelength {air} nm")
    vac = air
    for _ in range(8):
        n = refractive_index_air_morton2000(vac * 10.0)
        vac = air * n
    # Verify closure.
    if abs(vacuum_to_air_nm(vac) - air) > 1e-8:
        raise ValueError("air_to_vacuum_nm failed to converge")
    return vac


# ---------------------------------------------------------------------------
# Frozen windows
# ---------------------------------------------------------------------------

def oxygen_feature_windows() -> dict[str, dict]:
    """Predefined Oxygen windows. Boundaries frozen before residual inspection.

    Model integrals use vacuum bounds. Observation operators use the Morton
    air equivalents of those same bounds. Separate 630/636 windows are retained
    for localization, with oi_red_combined preferred when resolution blends them.
    """
    features = {
        "oi_green": PALACE_VACUUM_NM["oi_green"],
        "oi_630": PALACE_VACUUM_NM["oi_630"],
        "oi_636": PALACE_VACUUM_NM["oi_636"],
    }
    windows = {}
    for name, vac_c in features.items():
        vac_lo = vac_c - HALF_WIDTH_NM
        vac_hi = vac_c + HALF_WIDTH_NM
        air_c = vacuum_to_air_nm(vac_c)
        windows[name] = {
            "kind": "oxygen_local_excess",
            "comparability": "conditional",
            "vacuum_center_nm": vac_c,
            "air_center_nm": air_c,
            "vacuum_bounds_nm": [vac_lo, vac_hi],
            "air_bounds_nm": [vacuum_to_air_nm(vac_lo), vacuum_to_air_nm(vac_hi)],
            "half_width_nm": HALF_WIDTH_NM,
            "sideband_inner_nm": SIDEBAND_INNER_NM,
            "sideband_outer_nm": SIDEBAND_OUTER_NM,
            "included_palace_lines_vacuum_nm": [vac_c],
            "notes": (
                "Observed continuum-subtracted excess versus NSB Oxygen-only "
                "integral. Not a total-sky residual."
            ),
        }
    # Combined Or window: capture 630, 636 and the weak 639.35 line.
    vac_lo = PALACE_VACUUM_NM["oi_630"] - HALF_WIDTH_NM
    vac_hi = PALACE_VACUUM_NM["oi_639"] + HALF_WIDTH_NM
    windows["oi_red_combined"] = {
        "kind": "oxygen_local_excess",
        "comparability": "conditional",
        "vacuum_center_nm": None,
        "air_center_nm": None,
        "vacuum_bounds_nm": [vac_lo, vac_hi],
        "air_bounds_nm": [vacuum_to_air_nm(vac_lo), vacuum_to_air_nm(vac_hi)],
        "half_width_nm": None,
        "sideband_inner_nm": SIDEBAND_INNER_NM,
        "sideband_outer_nm": SIDEBAND_OUTER_NM,
        "sideband_reference_centers_air_nm": [
            vacuum_to_air_nm(PALACE_VACUUM_NM["oi_630"]),
            vacuum_to_air_nm(PALACE_VACUUM_NM["oi_636"]),
        ],
        "included_palace_lines_vacuum_nm": [
            PALACE_VACUUM_NM["oi_630"],
            PALACE_VACUUM_NM["oi_636"],
            PALACE_VACUUM_NM["oi_639"],
        ],
        "notes": (
            "Preferred red diagnostic when FORS1 resolution blends 630/636. "
            "Sidebands are built from the outer edges of the multiplet."
        ),
    }
    for name, (lo, hi) in CONTINUUM_CONTROL_AIR_NM.items():
        windows[name] = {
            "kind": "continuum_control_total",
            "comparability": "unsupported_as_airglow_residual",
            "vacuum_center_nm": None,
            "air_center_nm": None,
            "vacuum_bounds_nm": [air_to_vacuum_nm(lo), air_to_vacuum_nm(hi)],
            "air_bounds_nm": [lo, hi],
            "half_width_nm": None,
            "sideband_inner_nm": None,
            "sideband_outer_nm": None,
            "included_palace_lines_vacuum_nm": [],
            "notes": (
                "Observed total-sky integral versus NSB continuum Airglow only. "
                "Different physical component sets; descriptive scale comparison."
            ),
        }
    return windows


# ---------------------------------------------------------------------------
# Radiometric operators
# ---------------------------------------------------------------------------

def photon_density_per_nm(wave_angstrom, energy_flux):
    """Convert erg/s/cm2/A/arcsec2 samples to photons/cm2/ns/sr/nm."""
    wave = np.asarray(wave_angstrom, float)
    flux = np.asarray(energy_flux, float)
    if wave.shape != flux.shape or wave.ndim != 1 or wave.size < 2:
        raise ValueError("Matching finite spectral samples required")
    if not np.isfinite(wave).all() or not np.isfinite(flux).all():
        raise ValueError("Non-finite spectral samples")
    if np.any(np.diff(wave) <= 0):
        raise ValueError("Wavelength samples must increase")
    return flux * wave * 1e-8 / HC_ERG_CM * ARCSEC2_PER_SR * 1e-8


def integrate_density_nm(wave_nm, density_per_nm, lower_nm, upper_nm) -> float:
    """Trapezoidal integral of photon density over [lower, upper] in nm."""
    wave = np.asarray(wave_nm, float)
    dens = np.asarray(density_per_nm, float)
    lo, hi = float(lower_nm), float(upper_nm)
    if hi <= lo:
        raise ValueError("Empty integration window")
    if wave[0] > lo or wave[-1] < hi:
        raise ValueError("Spectrum does not cover the integration window")
    # Insert exact bounds so partial edge pixels are handled.
    mask = (wave > lo) & (wave < hi)
    grid = np.r_[lo, wave[mask], hi]
    values = np.interp(grid, wave, dens)
    return float(np.trapezoid(values, grid))


def linear_continuum_from_sidebands(
    wave_nm, density, center_nm, inner_nm=SIDEBAND_INNER_NM, outer_nm=SIDEBAND_OUTER_NM
):
    """Fit I(λ) = a + b λ on sidebands [c-outer,c-inner] ∪ [c+inner,c+outer]."""
    wave = np.asarray(wave_nm, float)
    dens = np.asarray(density, float)
    c = float(center_nm)
    left = (wave >= c - outer_nm) & (wave <= c - inner_nm)
    right = (wave >= c + inner_nm) & (wave <= c + outer_nm)
    selected = left | right
    if int(selected.sum()) < 4:
        raise ValueError("Insufficient sideband samples for continuum fit")
    # np.polyfit(x, y, 1) returns [slope, intercept].
    coeffs = np.polyfit(wave[selected], dens[selected], 1)
    slope, intercept = float(coeffs[0]), float(coeffs[1])
    return intercept, slope


def continuum_subtracted_excess(
    wave_angstrom,
    energy_flux,
    air_center_nm,
    half_width_nm=HALF_WIDTH_NM,
    inner_nm=SIDEBAND_INNER_NM,
    outer_nm=SIDEBAND_OUTER_NM,
) -> dict:
    """Local excess: ∫(I − continuum) over [c±half] with sideband linear continuum."""
    wave_nm = np.asarray(wave_angstrom, float) / 10.0
    dens = photon_density_per_nm(wave_angstrom, energy_flux)
    intercept, slope = linear_continuum_from_sidebands(
        wave_nm, dens, air_center_nm, inner_nm, outer_nm
    )
    lo = air_center_nm - half_width_nm
    hi = air_center_nm + half_width_nm
    continuum = intercept + slope * wave_nm
    excess_density = dens - continuum
    excess = integrate_density_nm(wave_nm, excess_density, lo, hi)
    continuum_integral = integrate_density_nm(wave_nm, continuum, lo, hi)
    total = integrate_density_nm(wave_nm, dens, lo, hi)
    return {
        "excess": excess,
        "total": total,
        "continuum_integral": continuum_integral,
        "continuum_intercept": intercept,
        "continuum_slope": slope,
        "air_bounds_nm": [lo, hi],
    }


def multiplet_continuum_subtracted_excess(
    wave_angstrom,
    energy_flux,
    air_bounds_nm,
    reference_centers_air_nm,
    inner_nm=SIDEBAND_INNER_NM,
    outer_nm=SIDEBAND_OUTER_NM,
) -> dict:
    """Excess over a multiplet window using outer sidebands of the end members."""
    wave_nm = np.asarray(wave_angstrom, float) / 10.0
    dens = photon_density_per_nm(wave_angstrom, energy_flux)
    c_lo = min(reference_centers_air_nm)
    c_hi = max(reference_centers_air_nm)
    # Left of blue-most and right of red-most feature.
    left = (wave_nm >= c_lo - outer_nm) & (wave_nm <= c_lo - inner_nm)
    right = (wave_nm >= c_hi + inner_nm) & (wave_nm <= c_hi + outer_nm)
    selected = left | right
    if int(selected.sum()) < 4:
        raise ValueError("Insufficient multiplet sideband samples")
    coeffs = np.polyfit(wave_nm[selected], dens[selected], 1)
    slope, intercept = float(coeffs[0]), float(coeffs[1])
    lo, hi = map(float, air_bounds_nm)
    continuum = intercept + slope * wave_nm
    excess = integrate_density_nm(wave_nm, dens - continuum, lo, hi)
    return {
        "excess": excess,
        "total": integrate_density_nm(wave_nm, dens, lo, hi),
        "continuum_integral": integrate_density_nm(wave_nm, continuum, lo, hi),
        "continuum_intercept": intercept,
        "continuum_slope": slope,
        "air_bounds_nm": [lo, hi],
    }


def measured_band_integral(wavelength_angstrom, energy, lower, upper):
    """Exact integral of lambda times piecewise-linear I_E, retaining noisy negatives.

    erg/s/cm2/A/arcsec2 -> photons/cm2/ns/sr. On each clipped interval,
    Simpson integration is exact for the quadratic product lambda*I_E.
    This differs from trapezoid integration of pre-converted photon samples.
    """
    from scipy.integrate import simpson

    wave = np.asarray(wavelength_angstrom, float)
    flux = np.asarray(energy, float)
    if (
        wave.ndim != 1 or wave.size < 2 or flux.shape != wave.shape
        or not np.isfinite(wave).all() or not np.isfinite(flux).all()
        or np.any(np.diff(wave) <= 0)
        or not wave[0] <= lower * 10 < upper * 10 <= wave[-1]
    ):
        raise ValueError("Finite increasing full-coverage spectral samples required")
    edges = np.r_[lower * 10, wave[(wave > lower * 10) & (wave < upper * 10)], upper * 10]
    # Each Simpson panel spans one original linear segment, including clipped edges.
    grid = np.sort(np.r_[edges, (edges[:-1] + edges[1:]) / 2])
    area = simpson(grid * np.interp(grid, wave, flux), x=grid)
    return float(area * 1e-8 / HC_ERG_CM * ARCSEC2_PER_SR * 1e-9)

# ---------------------------------------------------------------------------
# Synthetic instrumental LSF helpers (tests / leakage accounting)
# ---------------------------------------------------------------------------

def gaussian_lsf_kernel(wavelength_nm, center_nm, fwhm_nm):
    """Unit-integral Gaussian line-spread kernel on a wavelength grid."""
    if not (np.isfinite(fwhm_nm) and fwhm_nm > 0):
        raise ValueError("FWHM must be finite and positive")
    sigma = float(fwhm_nm) / (2.0 * np.sqrt(2.0 * np.log(2.0)))
    wave = np.asarray(wavelength_nm, float)
    kernel = np.exp(-0.5 * ((wave - center_nm) / sigma) ** 2)
    area = np.trapezoid(kernel, wave)
    if area <= 0:
        raise ValueError("Degenerate LSF kernel")
    return kernel / area


def convolve_line_flux(wavelength_nm, line_center_nm, line_flux, fwhm_nm):
    """Spread an integrated line flux into a density with a normalized Gaussian LSF."""
    kernel = gaussian_lsf_kernel(wavelength_nm, line_center_nm, fwhm_nm)
    return kernel * float(line_flux)


# ---------------------------------------------------------------------------
# Statistics
# ---------------------------------------------------------------------------

def signed_residuals(observed, predicted):
    """Signed residual = predicted − observed (positive = model high)."""
    obs = np.asarray(observed, float)
    pred = np.asarray(predicted, float)
    if obs.shape != pred.shape:
        raise ValueError("Observation/prediction length mismatch")
    return pred - obs


def mae_bias(residuals):
    resid = np.asarray(residuals, float)
    return {
        "mae": float(np.mean(np.abs(resid))),
        "bias": float(np.mean(resid)),
        "n": int(resid.size),
    }


def night_cluster_ci(values, nights, statistic, repetitions=BOOTSTRAP, seed=SEED):
    """Resample whole nights; preserve matched pairs inside each night draw."""
    values = np.asarray(values, float)
    nights = np.asarray(nights)
    unique = np.unique(nights)
    rng = np.random.default_rng(seed)
    estimates = []
    for _ in range(repetitions):
        draw = rng.choice(unique, size=unique.size, replace=True)
        sample = np.concatenate([values[nights == night] for night in draw])
        estimates.append(statistic(sample))
    lo, hi = np.quantile(estimates, [0.025, 0.975])
    return [float(lo), float(hi)]


# ---------------------------------------------------------------------------
# Frozen Patat cohort + NSB evaluation
# ---------------------------------------------------------------------------

def verify_sha256(path: Path, expected: str) -> None:
    if not path.is_file():
        raise FileNotFoundError(f"Input missing: {path}")
    with path.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != expected:
        raise ValueError(f"{path}: expected sha256 {expected}, got {actual}")


def solar_daily(path: Path) -> dict[date, float]:
    daily: dict[date, float] = {}
    seen: set[date] = set()
    for line in path.read_text().splitlines():
        fields = line.split()
        if not fields:
            continue
        key = fields[0]
        if len(key) != 8 or not key.isdigit() or len(fields) > 2:
            raise ValueError(f"Malformed solar observation: {line!r}")
        day = date(int(key[:4]), int(key[4:6]), int(key[6:8]))
        if day in seen:
            raise ValueError(f"Duplicate observed day {day}")
        seen.add(day)
        if len(fields) == 1 or fields[1] == ".":
            continue
        value = float(fields[1])
        if not np.isfinite(value) or value <= 0:
            raise ValueError(f"Invalid observed F10.7 for {day}")
        daily[day] = value
    if not daily:
        raise ValueError("No usable daily observed solar data")
    return daily


def centered_f27(daily: dict[date, float], day: date) -> float:
    days = [day + timedelta(days=i) for i in range(-13, 14)]
    if any(d not in daily for d in days):
        raise ValueError(f"Incomplete centered 27-day window for {day}")
    sample = np.array([daily[d] for d in days], float)
    if not np.isfinite(sample).all() or np.any(sample <= 0):
        raise ValueError("Invalid solar input")
    return float(np.mean(sample))


# Frozen registration from observational-validation.ipynb at 017eae0.
# Runtime reproduction does not read/execute the notebook. Parity tests protect
# the cuts, input identities and measurement operator against accidental drift.
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


def load_cohort(archive, *, enforce_registration=True):
    from astropy.io import fits

    rows, rejected = [], {}
    with tarfile.open(archive) as tar:
        members = sorted(
            (m for m in tar.getmembers() if m.isfile() and m.name.startswith("fxSKY") and m.name.endswith(".fits")),
            key=lambda m: m.name,
        )
        if len(members) != 1189 or len({m.name for m in members}) != len(members):
            raise ValueError("Registered FITS inventory changed")
        for member in members:
            with fits.open(BytesIO(tar.extractfile(member).read())) as hdus:
                hdr, flux = hdus[0].header, np.asarray(hdus[0].data, float)
                if hdr["BUNIT"].strip() != "erg/s/cm2/A/arcsec2":
                    raise ValueError(f"Unexpected flux unit: {member.name}")
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
                    raise ValueError(f"Nonfinite metadata: {member.name}")
                row.update(
                    id=member.name,
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
            row["observed"] = measured_band_integral(wave, flux, 365.0, 650.0)
            row["split"] = "calibration" if row["night"] in CALIBRATION_NIGHTS else "evaluation"
            if not np.isfinite(row["observed"]) or row["observed"] <= 0:
                raise ValueError("Nonpositive integrated observational radiance")
            rows.append(row)
    identities = "\n".join(f"{r['id']}:{r['night']}:{r['split']}" for r in rows).encode()
    identity_sha = hashlib.sha256(identities).hexdigest()
    if enforce_registration:
        if identity_sha != COHORT_SHA or len(rows) != 103:
            raise ValueError(
                "Selected observation identities/night partition differ from registration"
            )
    return rows, {
        "selected": len(rows),
        "nights": len({r["night"] for r in rows}),
        "excluded_ids": rejected,
        "identity_sha256": identity_sha,
    }

def observe_windows(row: dict, windows: dict) -> dict[str, dict]:
    """Apply frozen observational operators to one spectrum."""
    out = {}
    for name, window in windows.items():
        if window["kind"] == "oxygen_local_excess":
            if name == "oi_red_combined":
                result = multiplet_continuum_subtracted_excess(
                    row["wave"],
                    row["energy"],
                    window["air_bounds_nm"],
                    window["sideband_reference_centers_air_nm"],
                )
            else:
                result = continuum_subtracted_excess(
                    row["wave"], row["energy"], window["air_center_nm"]
                )
            out[name] = {
                "quantity": "continuum_subtracted_excess",
                "value": result["excess"],
                "total_in_window": result["total"],
                "continuum_integral": result["continuum_integral"],
                "unit": UNIT,
                "air_bounds_nm": result["air_bounds_nm"],
            }
        else:
            lo, hi = window["air_bounds_nm"]
            value = measured_band_integral(row["wave"], row["energy"], lo, hi)
            out[name] = {
                "quantity": "total_sky_band_integral",
                "value": value,
                "unit": UNIT,
                "air_bounds_nm": [lo, hi],
            }
    return out


def predict_nsb_windows(query, windows, sfu=None) -> dict[str, dict]:
    """NSB Airglow-only window integrals; Oxygen-only = Automatic − continuum."""
    import nsb
    configs = {
        "continuum": nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(
            nsb.AirglowModel.PARANAL_PALACE_V1
        ),
        "automatic": nsb.NsbModelConfig.generic_clear_sky(),
    }
    if sfu is not None:
        configs = {
            key: cfg.with_solar_radio_flux_sfu(float(sfu)) for key, cfg in configs.items()
        }
    evaluators = {key: nsb.NsbEvaluator(cfg) for key, cfg in configs.items()}
    out = {}
    for name, window in windows.items():
        lo, hi = window["vacuum_bounds_nm"]
        cont = float(evaluators["continuum"].integrate_band(query, lo, hi))
        auto = float(evaluators["automatic"].integrate_band(query, lo, hi))
        oxygen_only = auto - cont
        if window["kind"] == "oxygen_local_excess":
            out[name] = {
                "quantity": "nsb_oxygen_only_band_integral",
                "value": oxygen_only,
                "automatic_airglow": auto,
                "continuum_airglow": cont,
                "unit": UNIT,
                "vacuum_bounds_nm": [lo, hi],
                "comparability": "conditional",
            }
        else:
            out[name] = {
                "quantity": "nsb_continuum_airglow_band_integral",
                "value": cont,
                "automatic_airglow": auto,
                "continuum_airglow": cont,
                "oxygen_only": oxygen_only,
                "unit": UNIT,
                "vacuum_bounds_nm": [lo, hi],
                "comparability": "unsupported_as_airglow_residual",
            }
    return out


def summarize_admissible(observed, predicted, nights, eval_mask):
    obs = np.asarray(observed, float)[eval_mask]
    pred = np.asarray(predicted, float)[eval_mask]
    night = np.asarray(nights)[eval_mask]
    resid = signed_residuals(obs, pred)
    summary = mae_bias(resid)
    summary.update({
        "mean_observed": float(np.mean(obs)),
        "mean_predicted": float(np.mean(pred)),
        "nights": int(len(np.unique(night))),
        "bias_ci95": night_cluster_ci(resid, night, np.mean),
        "mae_ci95": night_cluster_ci(np.abs(resid), night, np.mean),
    })
    # Relative MAE only when the observed excess scale is meaningful.
    if np.all(np.abs(obs) > 1e-6):
        relative = np.abs(resid) / np.abs(obs)
        summary["relative_mae"] = float(np.mean(relative))
        summary["relative_mae_ci95"] = night_cluster_ci(relative, night, np.mean)
    else:
        summary["relative_mae"] = None
        summary["relative_mae_ci95"] = None
        summary["relative_mae_omitted_reason"] = (
            "Observed denominator too small or mixed-sign for stable relative MAE"
        )
    return summary


COMPARABILITY_MATRIX = [
    {
        "comparison": "Observed continuum-subtracted O I excess vs NSB Oxygen-only",
        "quantity": "Band-integrated photon radiance excess / Oxygen line contribution",
        "units": UNIT,
        "spectral_frame": "Observation air (Morton 2000); model vacuum (PALACE)",
        "instrumental_operator": (
            "Linear sideband continuum 3–5 nm from feature; integrate excess over ±2.5 nm "
            "(combined Or: multiplet window with outer sidebands). No LSF convolution: "
            "integrated flux comparison; FORS1 300V FWHM not in FITS headers."
        ),
        "included_components_observation": (
            "Local spectral excess above fitted continuum (Oxygen + blends + residual "
            "continuum mismatch + unmodeled lines)"
        ),
        "included_components_model": "NSB PALACE Oxygen delta-line contribution only",
        "missing_components": (
            "Na, OH, O2, N; any blend inside the window; continuum renormalization"
        ),
        "status": "conditional",
        "rationale": (
            "Physically motivated scale comparison used in oxygen-and-calibration-v1; "
            "not an independent absolute Oxygen admission (#280)."
        ),
    },
    {
        "comparison": "Observed total-sky continuum-control bands vs NSB continuum Airglow",
        "quantity": "Band-integrated photon radiance",
        "units": UNIT,
        "spectral_frame": "Observation air; model vacuum bounds via Morton",
        "instrumental_operator": "Registered exact λ·I_E Simpson band integral",
        "included_components_observation": "Total sky (Airglow+Zodiacal+Starlight+…)",
        "included_components_model": "NSB continuum Airglow only",
        "missing_components": "Zodiacal, Starlight, Oxygen lines, other emitters",
        "status": "unsupported_as_airglow_residual",
        "rationale": (
            "Descriptive scale comparison only; must not be labelled Airglow prediction error."
        ),
    },
    {
        "comparison": "Observed total sky vs NSB Automatic Airglow spectral density",
        "quantity": "Full-resolution spectral MAE",
        "units": UNIT + " / nm or similar",
        "spectral_frame": "mixed",
        "instrumental_operator": "n/a",
        "included_components_observation": "Total sky",
        "included_components_model": "Airglow only",
        "missing_components": "Zodiacal, Starlight; Starlight lacks arbitrary-band spectrum",
        "status": "unsupported",
        "rationale": "Inequivalent physical quantities; rejected under issue #296.",
    },
    {
        "comparison": "nsb2 isolated Oxygen emission groups",
        "quantity": "Oxygen-only spectral windows",
        "units": UNIT,
        "spectral_frame": "SkyCalc combined Airglow",
        "instrumental_operator": "n/a",
        "included_components_observation": "n/a",
        "included_components_model": "Combined SkyCalc Airglow (continuum+lines)",
        "missing_components": "Separable Oxygen channel",
        "status": "unsupported",
        "rationale": (
            "nsb2 exposes combined SkyCalc Airglow; Oxygen cannot be isolated without "
            "an admitted continuum channel."
        ),
    },
]


def build_artifact(args) -> dict:
    import nsb
    import siderust
    from astropy.time import Time
    from astropy.utils import data as astropy_data, iers

    astropy_data.conf.allow_internet = False
    iers.conf.auto_download = False
    verify_sha256(args.patat, PATAT_SHA)
    verify_sha256(args.solar, SOLAR_SHA)
    rows, selection = load_cohort(args.patat)
    if selection["identity_sha256"] != COHORT_SHA:
        raise ValueError("Cohort fingerprint changed")
    if (len(rows), len({r["night"] for r in rows}),
        sum(r["split"] == "evaluation" for r in rows),
        len({r["night"] for r in rows if r["split"] == "evaluation"})) != (103, 22, 94, 17):
        raise ValueError("Registered 103/22 + 94/17 partition changed")

    daily = solar_daily(args.solar)
    for row in rows:
        day = Time(row["mjd"], format="mjd").to_datetime().date()
        row["f27"] = centered_f27(daily, day)

    windows = oxygen_feature_windows()
    observer = siderust.Observer(
        lon_deg=SITE["longitude"], lat_deg=SITE["latitude"], height_m=SITE["height"],
    )

    observed = {name: [] for name in windows}
    native = {name: [] for name in windows}
    matched = {name: [] for name in windows}
    nights = []
    splits = []
    ids = []
    broadband_check = []

    for i, row in enumerate(rows):
        obs = observe_windows(row, windows)
        query = nsb.PointQuery(
            observer,
            Time(row["mjd"], format="mjd", scale="utc").to_datetime(timezone=UTC),
            ICRS(ra_deg=row["ra"], dec_deg=row["dec"]),
            components=nsb.ComponentMask.AIRGLOW,
        )
        nat = predict_nsb_windows(query, windows)
        mat = predict_nsb_windows(query, windows, sfu=row["f27"])
        for name in windows:
            observed[name].append(obs[name]["value"])
            native[name].append(nat[name]["value"])
            matched[name].append(mat[name]["value"])
        nights.append(row["night"])
        splits.append(row["split"])
        ids.append(row["id"])
        # Preserve registered broadband observation; do not recompute model totals here.
        broadband_check.append(row["observed"])
        if (i + 1) % 25 == 0 or i + 1 == len(rows):
            print(f"spectra {i + 1}/{len(rows)}", flush=True)

    eval_mask = np.array([s == "evaluation" for s in splits])
    nights_arr = np.array(nights)
    f27 = np.array([r["f27"] for r in rows], float)
    within = (f27 >= 67) & (f27 <= 166)

    # Sanity: oxygen-doc mean excesses over all 103 spectra.
    reference_means = {
        "oi_green": 0.03055,
        "oi_630": 0.02644,
        "oi_636": 0.00873,
    }

    per_window = {}
    for name, window in windows.items():
        entry = {
            "window": window,
            "n_spectra_all": 103,
            "n_nights_all": 22,
            "n_spectra_evaluation": 94,
            "n_nights_evaluation": 17,
            "mean_observed_all": float(np.mean(observed[name])),
            "mean_nsb_oxygen_or_continuum_native_all": float(np.mean(native[name])),
            "mean_nsb_oxygen_or_continuum_matched_all": float(np.mean(matched[name])),
        }
        if window["comparability"] == "conditional":
            entry["native_operational"] = summarize_admissible(
                observed[name], native[name], nights_arr, eval_mask
            )
            entry["matched_centered_f27_diagnostic"] = summarize_admissible(
                observed[name], matched[name], nights_arr, eval_mask
            )
            # Within-domain matched diagnostic (issue #295 context).
            entry["matched_within_palace_training"] = summarize_admissible(
                observed[name], matched[name], nights_arr, eval_mask & within
            )
            if name in reference_means:
                entry["oxygen_doc_mean_excess_reference"] = reference_means[name]
                entry["oxygen_doc_mean_excess_agreement"] = abs(
                    entry["mean_observed_all"] - reference_means[name]
                ) < 5e-4
        else:
            entry["interpretation"] = (
                "Unsupported as an Airglow residual. Means are retained only as a "
                "descriptive scale comparison between total-sky observations and "
                "NSB continuum Airglow."
            )
            entry["metrics_omitted_reason"] = window["comparability"]
        per_window[name] = entry

    # Broadband observation fingerprint (unchanged cohort radiance).
    broadband_mean = float(np.mean(broadband_check))
    return {
        "schema": PROTOCOL_VERSION,
        "issue": 296,
        "outcome": "B_restricted_admissible_comparison",
        "unit": UNIT,
        "wavelength_conversion": {
            "formula": "Morton 2000 / IAU optical vac↔air",
            "conditions": "dry air, 15 C, 760 Torr",
            "domain_angstrom": [2000, 10000],
        },
        "lsf_treatment": {
            "applied_to_model": False,
            "reason": (
                "FORS1 resolving power / FWHM is not recorded in the Patat FITS headers. "
                "Literature FORS1+300V resolving power is approximately 440 "
                "(FWHM ~1.3 nm near 558 nm). Windows of ±2.5 nm therefore enclose most "
                "of a Gaussian line core. Comparisons use integrated excess / line flux, "
                "not unconvolved peak height."
            ),
            "literature_resolving_power_approx": 440,
            "synthetic_leakage_tests": "scripts/tests/test_spectral_window_diagnostics.py",
        },
        "inputs": {
            "patat_sha256": PATAT_SHA,
            "solar_sha256": SOLAR_SHA,
            "cohort_sha256": COHORT_SHA,
            "calibration_nights": sorted(CALIBRATION_NIGHTS),
            "selection_identity_sha256": selection["identity_sha256"],
        },
        "protocol": {
            "half_width_nm": HALF_WIDTH_NM,
            "sideband_inner_nm": SIDEBAND_INNER_NM,
            "sideband_outer_nm": SIDEBAND_OUTER_NM,
            "bootstrap_repetitions": BOOTSTRAP,
            "bootstrap_seed": SEED,
            "signed_residual": "predicted - observed",
            "solar_scenarios": {
                "native_operational": "NSB Automatic climatology / monthly planning input",
                "matched_centered_f27_diagnostic": (
                    "Retrospective centered 27-day observed F10.7; not operational skill"
                ),
            },
            "windows_frozen_before_residuals": True,
        },
        "comparability_matrix": COMPARABILITY_MATRIX,
        "palace_vacuum_lines_nm": PALACE_VACUUM_NM,
        "broadband_observation_mean_365_650": broadband_mean,
        "broadband_observation_mean_reference": 0.32425,
        "per_window": per_window,
        "limitations": [
            "Not an independent absolute Oxygen admission (issue #280).",
            "Patat spectra informed Oxygen architecture; development-cohort diagnostics only.",
            "Local excess is not a pure species measurement; blends and continuum-fit residuals remain.",
            "Starlight lacks an admitted arbitrary-band spectral decomposition.",
            "nsb2 cannot isolate Oxygen from combined SkyCalc Airglow.",
            "F10.7 matched scenario is retrospective (#295); most Patat nights are outside PALACE 67–166 sfu.",
            "No model coefficients, defaults or broadband predictions were altered.",
        ],
        "unsupported_comparisons": [
            c["comparison"] for c in COMPARABILITY_MATRIX if c["status"] == "unsupported"
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--patat", type=Path, required=True)
    parser.add_argument("--solar", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ARTIFACT)
    parser.add_argument(
        "--check-existing",
        action="store_true",
        help="Regenerate and require bit-stable agreement with the committed artifact",
    )
    args = parser.parse_args()
    artifact = build_artifact(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(artifact, indent=2, sort_keys=True) + "\n"
    if args.check_existing:
        existing = args.output.read_text()
        if existing != text:
            raise SystemExit("Regenerated artifact disagrees with committed file")
        print("Artifact check OK")
        return
    args.output.write_text(text)
    print(f"Wrote {args.output}")


if __name__ == "__main__":
    main()
