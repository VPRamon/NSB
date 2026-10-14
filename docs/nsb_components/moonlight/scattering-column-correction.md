# Correction of the Jones-inspired scattering column

The earlier implementation multiplied the optical-depth-weighted phase function
by a second scattering probability, `(1-exp(-tau X_target))`. For fixed source
and geometry, scaling both optical depths by s therefore made the sky radiance
quadratic in s. First-order single scattering must be linear.

Jones et al. (2013), A&A 560 A91, Eq. 6:
https://doi.org/10.1051/0004-6361/201322433

For a shared vertical optical-depth coordinate u (0 at the observer, 1 at the
atmosphere top), the plane-parallel reduction is

    I_lambda = E_moon,lambda X_target (tau_R p_R + w_eff tau_M p_M)
               integral_0^1 exp[-tau (X_moon (1-u) + X_target u)] du.

The existing symmetric mean-path transmission is a midpoint approximation to
this integral. Its column multiplier is X_target, not another probability.
At equal airmasses it is exact for the shared-profile slab. With unequal
airmasses its relative quadrature error is order [tau (X_moon-X_target)]^2.
Rayleigh p_R integrates to one. The admitted Mie table P_M integrates to
4*pi (see its header and mie-phase-validation.md), so p_M=P_M/(4*pi)
is the phase function per steradian. The original empirical coefficient 0.05
was defined against P_M; with normalized p_M its equivalent weight is
w_eff=4*pi*0.05, about 0.6283. The code now expresses this normalization
explicitly, preserving the inherited product and all calibration choices.
Dividing by 4*pi while keeping a normalized-phase weight of 0.05 would
retune the model, rather than merely correct its units. Siderust's historical
lunar helper returned numerical beam irradiance despite carrying a radiance
marker. NSB now requires released Siderust 0.14.1 and uses its explicitly typed
irradiance API, including the inverse-square source correction. The typed kernel
converts this beam to sky radiance once, before photon conversion.

The Jones path also now uses the shared local-pressure Rayleigh helper, as do
airglow and direct transport. Local pressure already describes the molecular
column (hydrostatic column mass p/g). Passing altitude again had reduced it
by exp(-h/H). At 2.635 km and H=8 km that extra factor was 0.71937.

This is a correction of NSB's reduced model, not an implementation of Jones's
full spherical radiative-transfer solver. The empirical Mie weight 0.05 and
admitted multiple-scattering LUT are unchanged. The LUT is fixed to its
reference atmosphere: a scaled-depth experiment tests the exponent but cannot
validate the absolute first-order coefficient of the LUT. In particular, this
LUT does not tend to unity as the caller's optical depth tends to zero.
Its atmospheric portability remains unresolved; no new calibration is claimed.

Analytical tests cover zero depth, linear thin limits, pure molecular/aerosol
contributions, and independent numerical slab integrals at several wavelengths
and separations. An equivalent-local-pressure profile test prevents duplicated
altitude dilution. Snapshot tuples were regenerated only after these invariants
passed; old and new outputs are retained in the investigation bundle.

At phase 85.5 deg, separation 97.523 deg, Moon/target zenith 36/60 deg,
384400 km and the local Paranal profile, band radiance changes from
0.08475705829583428 to 0.48493586510203146 photons cm^-2 ns^-1 sr^-1
with the published Siderust source. This is a model correction, not an observed
accuracy estimate. Scalar snapshots at the mean lunar distance are unchanged
by the separate upstream distance fix.

The native artifact's normalization admission gate uses trapezoidal quadrature
of P(theta)*sin(theta), with maximum relative error 7.371e-5. Integrating the
runtime's piecewise-linear P(theta) instead gives a maximum 1.685e-4 deviation
from unit normalization after division by 4*pi (about 0.017%). A separate
independent closed-form cell integral verifies the runtime quadrature. This
small interpolation approximation is retained; it cannot explain the large
cross-model residuals. The unit-convention audit does not renormalize the data.
