# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Offline dense linear-algebra oracle for NSB's cell/ordinate transport operator.

This script intentionally solves a *coupled stationary linear system* through
numpy.linalg.solve, not Rust-style successive orders. The reference validates
the discrete higher-order recurrence, not the continuum atmospheric solution:
the same angular/vertical discretization cannot establish discretization error.
No network, model fitting, or runtime dependency on Python is involved.

Run with: python3 docs/validation/multiple-scattering-matrix.py
Requires a local NumPy installation; checked-in numerical fixtures remain usable
without Python. The calculation uses only an analytical isotropic phase law.
"""

from math import exp, expm1, pi
import numpy as np
from numpy.polynomial.legendre import leggauss

RADIANCE = 0.025  # photons/(cm^2 ns sr nm) at TOA
TAU_EXT = 0.14  # aerosol extinction 0.12 + molecular absorption 0.02
TAU_SCA = 0.084  # 0.12 aerosol extinction * SSA 0.7
LAYER_METERS = 1000.0
MU_VIEW = 0.8
MU_ORDER = 4
PHI_ORDER = 8
CELLS = 4


def calculate():
    nodes, weights = leggauss(MU_ORDER)
    nodes = 0.5 * (nodes + 1.0)
    weights = 0.5 * weights
    mu = np.array([
        sign * x
        for sign in (+1, -1)
        for x in nodes
        for _ in range(PHI_ORDER)
    ], dtype=np.float64)
    solid_angle = np.array([
        w * 2 * pi / PHI_ORDER
        for _ in (+1, -1)
        for w in weights
        for _ in range(PHI_ORDER)
    ], dtype=np.float64)
    directions = mu.size
    unknowns = CELLS * directions
    dt = TAU_EXT / CELLS
    beta_ext = TAU_EXT / LAYER_METERS
    beta_sca = TAU_SCA / LAYER_METERS

    # T maps an arbitrary constant-in-cell volume source to interior midpoint
    # intensities. It is a Green matrix from the formal transport solution.
    green = np.zeros((unknowns, unknowns))
    for direction, cosine in enumerate(mu):
        for target in range(CELLS):
            for upstream in range(CELLS):
                if (cosine > 0 and upstream < target) or (
                    cosine < 0 and upstream > target
                ):
                    continue
                optical = dt / abs(cosine)
                if target == upstream:
                    response = -expm1(-optical / 2.0) / beta_ext
                else:
                    response = (
                        -expm1(-optical)
                        * exp(-(abs(target - upstream) - 0.5) * optical)
                        / beta_ext
                    )
                green[target * directions + direction,
                      upstream * directions + direction] = response

    # Analytical isotropic P=1/(4*pi) angular quadrature. A=L_green * scatter.
    scatter = np.zeros((unknowns, unknowns))
    phase_weight = beta_sca * solid_angle / (4 * pi)
    for j in range(CELLS):
        scatter[
            j * directions:(j + 1) * directions,
            j * directions:(j + 1) * directions,
        ] = phase_weight[np.newaxis, :]
    operator = green @ scatter
    direct = np.zeros(unknowns)
    for j in range(CELLS):
        for index, cosine in enumerate(mu):
            if cosine > 0:
                direct[j * directions + index] = RADIANCE * exp(
                    -(CELLS - j - 0.5) * dt / cosine
                )

    # All-order diffuse field solves (I-A) D = A I_TOA.
    one_scatter_field = operator @ direct
    diffuse = np.linalg.solve(np.eye(unknowns) - operator, one_scatter_field)

    # A separate ground ray integrates source from each cell with exact
    # constant-source formal solution. Surface inflow is identically zero.
    ground = np.array([
        beta_sca / beta_ext
        * (-expm1(-dt / MU_VIEW))
        * exp(-j * dt / MU_VIEW)
        * angle / (4 * pi)
        for j in range(CELLS)
        for angle in solid_angle
    ])
    first = float(ground @ direct)
    higher = float(ground @ diffuse)
    observed_direct = RADIANCE * exp(-TAU_EXT / MU_VIEW)
    return first, higher, observed_direct


if __name__ == "__main__":
    first, higher, transmitted = calculate()
    print(f"first={first:.17g}")
    print(f"second_and_higher={higher:.17g}")
    print(f"direct={transmitted:.17g}")
    print(f"total={transmitted + first + higher:.17g}")
