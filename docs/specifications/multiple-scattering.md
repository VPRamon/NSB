# Experimental scalar multiple-scattering reference (draft)

Status: opt-in numerical implementation under scientific review. This is **not**
an admitted Starlight/Gaia result and must not be used as observational accuracy
evidence. It does not alter the NSB evaluator or production defaults.

## Physical contract

A horizontally homogeneous, vertically layered and finite plane-parallel
atmosphere has upstream Siderust extinction and Rayleigh/aerosol scattering
coefficients. Optica supplies normalized Rayleigh and aerosol phase densities
in inverse steradians. At fixed vacuum wavelength, photon directions are the
negatives of Horizontal sky vectors (local azimuth north clockwise). The upper
inflow boundary supplies full-hemisphere top-of-atmosphere celestial spectral
radiance; the black, nonemitting, nonreflecting lower boundary supplies zero
upward photons. Both upward and downward directions are retained after the
first scattering. Observer height is the first Siderust layer edge.

There is no airglow, ground reflection, emission, polarization, refraction,
spherical geometry, instrument response, or empirical correction. Layers are
assumed optically homogeneous as represented by Siderust. At present, only a
monochromatic smooth-illumination research domain is implemented.

## Numerical method

The diffuse radiance at scattering order n+1 obeys

    S_{n+1}(z,u) = beta_R(z) integral P_R(u,v) I_n(z,v) dv
                + beta_A(z) integral P_A(u,v) I_n(z,v) dv.

At order 0, the upper-boundary illumination is transmitted through the
Siderust optical column without scattering. A symmetric, full-sphere product
quadrature (Gauss-Legendre positive zenith cosines, two hemispheres, periodic
azimuth) represents the interior angular field. Within each homogeneous layer,
equal-height cells carry constant approximations to the diffuse source. Each
order is transported along both propagation hemispheres with the formal exact
constant-source solution. The requested ground view is separately swept with
its exact geometric direction; it need not equal an ordinate.

The method **does not** multiply first-order radiance by an estimated
enhancement. The returned fields distinguish direct, single, sum of second and
higher orders, total scattered, and total observed radiance.

## Convergence, bounds, and limitations

Four independently evaluated configurations compare base, angular-double,
vertical-double and joint-double resolutions. Each computes its own
scattering-order tail. The discrete positive scattering operator is
conservatively bounded by the maximum single-scattering albedo, maximum
discrete phase-row integral and finite-column escape probability. When that
bound is not strictly below one, the solver refuses to claim convergence.
A geometric bound of the remaining Neumann-series tail is checked at each
iteration. Failure to meet an individual angular, vertical or order tolerance
returns an error, never a silently truncated sum.

Preflight checks use checked arithmetic and apply to **all four** planned
calculations, including iterations that might never run. Current caps: 250
million conservative work units and 16 MiB for phase/field workspaces.
If K is the number of vertical cells, D the number of internal directions,
and N the maximum scattering orders, the dominant work is O(N K D^2)
and storage O(D^2 + K D). Grid sizes, number of layers, wavelength, and
iterations are individually bounded.

The discrete phase integral audit rejects under-resolved forward peaks.
As with existing single scattering, agreement on two grids is a *diagnostic*,
not a rigorous angular/vertical error bound. The contraction bound applies to
the **discrete scattering-order remainder only**, not discretization errors,
atmospheric-parameter uncertainty, or physical calibration.

**Important outstanding work** before scientific acceptance:

- Conservative HEALPix pixel-area projection and isolated-bright-pixel tests;
  point-sampling a Starlight map is not equivalent.
- Separate full-native-bin spectral integration and wavelength convergence,
  preserving FlatPhotonDensity provenance and the 360–570 nm Gaia XP bins.
- Tight matched-physics first-order equivalence tests across Rayleigh, aerosol,
  mixed media and multiple layers.
- Independent discrete-ordinates or DISORT/PythonicDISORT higher-order fixtures
  from a different algorithm, with physical assumptions matched precisely.
- Frozen Gaia XP comparison under both current and historical optics/astrometry;
  historical 28.10% is a comparison datum, not a multiplier.
- Complete all-feature Clippy, Rust 1.89, wheel/CLI, coverage and GitHub Actions
  checks on the final SHA.

No source uncertainty, covariance or observational accuracy estimate is
available. Release #265 still blocks publishing until real Siderust registry
artifacts provide the required APIs. Do not change the existing Git dependency
pin to make this draft build.

Refs #187, #192, #263, #265.
