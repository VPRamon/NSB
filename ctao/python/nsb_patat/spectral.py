"""Linear measurement operator for calibrated sampled energy surface radiance.

This is a conversion of a measured spectrum, never a continuous reconstruction
of model bin integrals. Input values interpolate linearly; negative noisy samples
are retained. Instrument calibration and resolution must be admitted separately.
"""

from __future__ import annotations

import itertools
from dataclasses import dataclass

import astropy.units as u
import numpy as np
from astropy.constants import c, h
from numpy.typing import NDArray

ENERGY_RADIANCE = u.W / u.m**2 / u.sr / u.nm
PHOTON_RADIANCE = 1 / u.cm**2 / u.ns / u.sr


@dataclass(frozen=True)
class PhotonBands:
    """Separate sampling covariance from common fractional calibration error."""

    values: u.Quantity
    statistical_covariance: u.Quantity | None
    calibration_covariance: u.Quantity | None
    operator: NDArray[np.float64]


def photon_bands(
    wavelength: u.Quantity,
    spectral_radiance: u.Quantity,
    edges: u.Quantity,
    *,
    statistical_covariance: u.Quantity | None = None,
    calibration_fractional_sigma: float | None = None,
) -> PhotonBands:
    """Integrate I_lambda lambda/(hc), with exact linear-sample weights.

    Covariance describes sampled I_lambda, including any reduction correlations.
    A known shared calibration sigma gives a rank-one *separate* covariance;
    unknown calibration remains None, never an implicit zero or complete budget.
    """
    wave = np.asarray(wavelength.to_value(u.nm), dtype=float)
    flux = np.asarray(spectral_radiance.to_value(ENERGY_RADIANCE), dtype=float)
    band = np.asarray(edges.to_value(u.nm), dtype=float)
    for name, arr in (("wavelength", wave), ("edges", band)):
        if arr.ndim != 1 or len(arr) < 2 or not np.all(np.isfinite(arr)):
            raise ValueError(f"Finite 1D {name} required")
        if np.any(arr <= 0) or np.any(np.diff(arr) <= 0):
            raise ValueError(f"Positive strictly increasing {name} required")
    if flux.shape != wave.shape or not np.all(np.isfinite(flux)):
        raise ValueError("Finite matching spectral radiance required")
    if band[0] < wave[0] or band[-1] > wave[-1]:
        raise ValueError("Incomplete spectral coverage")
    matrix = np.zeros((len(band) - 1, len(wave)))
    # Two-point Gauss quadrature of lambda times each linear basis is exact.
    conversion = (1 * ENERGY_RADIANCE * u.nm**2 / (h * c)).to_value(PHOTON_RADIANCE)
    for row, (lower, upper) in enumerate(itertools.pairwise(band)):
        grid = np.r_[lower, wave[(wave > lower) & (wave < upper)], upper]
        for left, right in itertools.pairwise(grid):
            middle, half = (left + right) / 2, (right - left) / 2
            for node in (middle - half / np.sqrt(3), middle + half / np.sqrt(3)):
                index = min(int(np.searchsorted(wave, node, side="right")) - 1, len(wave) - 2)
                fraction = (node - wave[index]) / (wave[index + 1] - wave[index])
                matrix[row, index] += half * node * (1 - fraction) * conversion
                matrix[row, index + 1] += half * node * fraction * conversion
    values = matrix @ flux
    covariance = None
    if statistical_covariance is not None:
        cov = np.asarray(statistical_covariance.to_value(ENERGY_RADIANCE**2), dtype=float)
        if cov.shape != (len(wave), len(wave)) or not np.all(np.isfinite(cov)):
            raise ValueError("Finite matching covariance required")
        scale = float(np.max(abs(cov)))
        if scale:
            normalized = cov / scale
            if not np.allclose(normalized, normalized.T, rtol=0, atol=1e-12):
                raise ValueError("Symmetric covariance required")
            if np.linalg.eigvalsh(normalized).min() < -1e-12:
                raise ValueError("Positive semidefinite covariance required")
        covariance = (matrix @ cov @ matrix.T) * PHOTON_RADIANCE**2
    calibration = None
    if calibration_fractional_sigma is not None:
        sigma = calibration_fractional_sigma
        if not np.isfinite(sigma) or sigma < 0:
            raise ValueError("Nonnegative finite calibration sigma required")
        calibration = np.outer(values * sigma, values * sigma) * PHOTON_RADIANCE**2
    return PhotonBands(values * PHOTON_RADIANCE, covariance, calibration, matrix)
