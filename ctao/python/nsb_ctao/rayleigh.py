"""Rayleigh ↔ NSB photon-radiance conversions.

A Rayleigh is a column emission rate. For an optically thin, isotropic layer
viewed as surface brightness,

    1 R = 10^6 / (4 π) photons cm^-2 s^-1 sr^-1
        = 10^-3 / (4 π) photons cm^-2 ns^-1 sr^-1

NSB's PALACE oxygen asset uses the same convention (see NSB oxygen audit).
This helper never multiplies an already-integrated Rayleigh by an artificial
wavelength width.
"""

from __future__ import annotations

import math

PHOTONS_CM2_NS_SR_PER_RAYLEIGH = 1.0e-3 / (4.0 * math.pi)


def photons_per_ns_sr_from_rayleigh(rayleigh: float) -> float:
    if not math.isfinite(rayleigh):
        raise ValueError("Rayleigh intensity must be finite")
    return float(rayleigh) * PHOTONS_CM2_NS_SR_PER_RAYLEIGH


def rayleigh_from_photons_per_ns_sr(radiance: float) -> float:
    if not math.isfinite(radiance):
        raise ValueError("Radiance must be finite")
    return float(radiance) / PHOTONS_CM2_NS_SR_PER_RAYLEIGH
