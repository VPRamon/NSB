# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Offline independent scalar reference. Requires scipy==1.17.1.

Azimuth is eliminated analytically for Rayleigh and isotropic aerosol. The
remaining incoming-mu integral uses exponential integrals E_n (SciPy special,
not NSB's GL quadrature). The path integral uses adaptive QUADPACK with explicit
layer boundaries. No NSB implementation, GL rules, or divided-difference oracle
is imported. Illumination is 0.01*(lambda/450)*(1+0.4*mu) spectral photon radiance.
All fixtures are at 450 nm, output azimuth 23 degrees, mu_out=0.8; black ground,
finite plane-parallel layers, no ground illumination, no multiple scattering.
Coefficients in m^-1; heights absolute MSL meters. Outputs photons/(cm² ns sr nm).
"""
import json
from pathlib import Path
import scipy
from scipy.integrate import quad
from scipy.special import expn
from math import exp

assert scipy.__version__ == "1.17.1"
fixtures = []
for name, edges, layers in [
    ("homogeneous", [2100., 3100.], [[0.00007, 0.00012, 0.7, 0.00003]]),
    ("two-layer", [2100., 2500., 3100.], [[0.0001, 0., 0., 0.0002], [0., 0.0002, 0.9, 0.]]),
]:
    mu_o = 0.8
    ext = [r+a+m for r,a,ssa,m in layers]
    column = sum(e*(b-a) for e,a,b in zip(ext,edges,edges[1:]))
    total = 0.
    estimate = 0.
    for i, (r,a,ssa,m) in enumerate(layers):
        def source(z):
            below = sum(ext[j]*(edges[j+1]-edges[j]) for j in range(i)) + ext[i]*(z-edges[i])
            above = max(0., column-below)
            # Azimuth-averaged Rayleigh is quadratic in incoming mu.
            c0 = 1. + 0.5*(1.-mu_o**2)
            c2 = mu_o**2 - 0.5*(1.-mu_o**2)
            rayleigh = 3./8.*(c0*(expn(2,above)+0.4*expn(3,above)) + c2*(expn(4,above)+0.4*expn(5,above)))
            aerosol = 0.5*(expn(2,above)+0.4*expn(3,above))
            return 0.01*(r*rayleigh + a*ssa*aerosol)*exp(-below/mu_o)/mu_o
        value, error = quad(source, edges[i], edges[i+1], epsabs=1e-16, epsrel=1e-12, limit=300)
        total += value
        estimate += error
    fixtures.append(dict(name=name, edges_m=edges, layers=layers, mu_out=mu_o,
        wavelength_nm=450., direct=0.01*(1.+0.4*mu_o)*exp(-column/mu_o), single=total,
        quadpack_estimated_absolute_error=estimate, relative_test_tolerance=1e-4))
output = dict(reference="SciPy 1.17.1 scipy.special.expn + scipy.integrate.quad (QUADPACK)",
    generator="scripts/generate-single-scattering-reference.py", phase="Rayleigh unity/4pi + isotropic aerosol g=0",
    illumination="0.01*(lambda_nm/450)*(1+0.4*mu_in) photons/(cm2 ns sr nm)", fixtures=fixtures)
path = Path(__file__).resolve().parents[1]/"crates/nsb/tests/data/single-scattering/scipy-1.17.1.json"
path.write_text(json.dumps(output, indent=2)+"\n")
print(path.read_text())
