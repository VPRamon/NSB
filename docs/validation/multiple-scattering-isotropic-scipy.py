# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Independent isotropic scattering continuum oracle, offline only.

For a finite homogeneous plane-parallel black-ground atmosphere with
isotropic upper-hemisphere radiance and isotropic elastic scattering,
azimuth and zenith can be integrated analytically. The mean intensity
then obeys a Fredholm equation with an E1 exponential-integral kernel.
We solve that *independent* integral equation using SciPy's expn and a
dense direct linear solver, without NSB's discrete ordinates, source
iteration, or the Rust internal angular grid.

This verifies the scalar higher-order physics for the specified isotropic
case. It does not validate anisotropic phase functions or HEALPix transport.

Run: python3 docs/validation/multiple-scattering-isotropic-scipy.py
Requires NumPy and SciPy offline; neither is a production dependency.
"""

from math import exp
import numpy as np
from scipy.special import expn

TOA = 0.025  # photons / (cm^2 ns sr nm)
TAU = 0.14  # 0.12 aerosol extinction + 0.02 molecular absorption
SSA = 0.12 * 0.7 / TAU  # effective single scattering albedo
MU_VIEW = 0.8


def evaluate(cells: int) -> tuple[float, float, float]:
    edges = np.linspace(0.0, TAU, cells + 1)
    mid = 0.5 * (edges[1:] + edges[:-1])
    # J0(t) = I_TOA E2(t) / 2, the exact upper-hemisphere mean
    # intensity for isotropic illumination; t measures depth from TOA.
    unscattered_mean = 0.5 * TOA * expn(2, mid)

    # K_ij = SSA/2 integral_cell_j E1(|t_i-s|) ds.
    # The logarithmically singular E1 kernel is analytically integrated
    # over each source cell using dE2(x)/dx = -E1(x).
    interaction = np.empty((cells, cells))
    for i, target in enumerate(mid):
        below = edges[1:] <= target
        above = edges[:-1] >= target
        same = ~(below | above)
        interaction[i, below] = (
            expn(2, target - edges[1:][below])
            - expn(2, target - edges[:-1][below])
        )
        interaction[i, above] = (
            expn(2, edges[:-1][above] - target)
            - expn(2, edges[1:][above] - target)
        )
        interaction[i, same] = (
            2.0
            - expn(2, target - edges[:-1][same])
            - expn(2, edges[1:][same] - target)
        )
    interaction *= SSA / 2.0
    total_mean = np.linalg.solve(
        np.eye(cells) - interaction, unscattered_mean
    )
    # Exact formal integration of a constant-in-cell mean source along
    # the requested downward ground ray.
    gain = (
        SSA
        * (-np.expm1(-(edges[1:] - edges[:-1]) / MU_VIEW))
        * np.exp(-(TAU - edges[1:]) / MU_VIEW)
    )
    first = float(gain @ unscattered_mean)
    higher = float(gain @ (total_mean - unscattered_mean))
    direct = TOA * exp(-TAU / MU_VIEW)
    return first, higher, direct


if __name__ == "__main__":
    for n in (32, 64, 128, 256):
        first, higher, direct = evaluate(n)
        print(
            f"cells={n} first={first:.17g} "
            f"higher={higher:.17g} direct={direct:.17g}"
        )
