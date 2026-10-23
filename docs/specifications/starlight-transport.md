# Experimental spectral Starlight single scattering (#267)

`nsb::transport::single::starlight` connects the checksum-validated
`StarlightMap` to the #190 first-order transport kernel. This opt-in Rust API
does not change evaluator, Python or CLI predictions, integrated 300–650 nm
products, production admission, dependencies or scientific assets.

## Inputs and vertical datums

Construct `StarlightSky::new(map, geometry, [low, high],
WithinBinSpectrum::FlatPhotonDensity)` for one complete native bin. Iterate
`map.bin_edges_nm().windows(2)` for unions; report no total if any bin fails.
Partial bins, extrapolation and integrated-only maps fail. Every edge
must lie inside both 360–570 nm and the medium's wavelength domain. The existing
loader enforces checksum, complete finite nonnegative pixels, TOA origin and
vacuum wavelengths. Variable-width bins work; NESTED remains rejected upstream.

`StarlightGeometry.observer` is Siderust/AFFN WGS84 `Geodetic<ECEF>`: its height
is **ellipsoidal**. The atmosphere uses its independently declared vertical
datum (MSL for `AtmosphereProfile`), with its first edge defining the
plane-parallel observer origin. Numeric equality of these two heights is neither
required nor a consistency test. Both inputs are preserved; callers supply
physically consistent site/column information. No geoid conversion, inferred
orthometric height, altitude offset or layer-boundary adjustment occurs. Local
pressure remains a caller input with Siderust's existing Rayleigh semantics.

Supply explicit TT and UT1 for the same physical instant; no network EOP lookup
or UTC substitution occurs. Directions have geometric altitude and north-clockwise
azimuth. Siderust handles Horizontal ↔ true-equatorial-of-date with IAU 2006 GAST,
then date-dependent ICRS conversion. The fixed ICRS/Galactic step uses its
canonical `TransformFrame` API, also used by production Starlight. The pinned
dynamic frame catalog reverses that fixed rotation; an absolute IAU Galactic-pole
anchor supplements round-trip tests. Three upstream-transformed basis vectors
retain the rotation, including seam/poles. The #190 domain is output altitude
10–90°, finite plane-parallel column and black nonemitting ground.

Each source value is a containing pixel's **spatial average**, never a resolved
star, interpolated intensity, PSF or invented subpixel photometry. Direct-view
lookup uses its exact direction independently of angular quadrature.

## Spectral and angular integration

`FlatPhotonDensity` explicitly declares `I_lambda = I_bin / bin_width` in
photons/(cm² ns sr nm); it cannot recover a measured continuous Gaia SED.
Both public entry points integrate the **full native bin** using independently
doubled wavelength GL orders. Both direct and single changes must pass; angular
acceptance never substitutes for wavelength convergence.

`starlight_single_scattering` retains the #190 smooth-sky GL/periodic-azimuth
and independent angular/path/joint refinement. Containing-pixel discontinuities
can alias this rule; the original frozen-map nonconvergence remains a valid
reason to use the pixel-aware entry point.

`starlight_healpix_single_scattering` instead streams equal-area RING integration
grids at native NSIDE times a power-of-two subdivision. Each fine point inherits
its containing **native parent mean**; attenuation and Optica phase density are
evaluated at the fine direction. Refinement changes the kernel quadrature,
never the source model. HEALPix child centers partition each parent equally.
RING's first half contains one member of each antipodal pair. Selecting the
positive local-zenith member assigns exactly one fine solid angle per pair,
conserving the hemisphere measure 2π for any rotation. Horizon-crossing parents
are subdivided; the boundary itself has zero finite-column response. There is
no positive-altitude cutoff. Parent-area, independent visible-area, quarter-turn
seam symmetry and compatible-resolution tests check this construction.

The pixel path uses the exact antiderivative of the **same** #190 path integrand
in each Siderust homogeneous layer. With layer extinction τ, incoming/outgoing
cosines μi/μo, `a=τ/μi`, `b=τ/μo`, it multiplies the integrated constituent
scattering coefficient divided by μo by

```text
exp(-tau_above/μi - tau_below/μo - min(a,b))
    * [-expm1(-abs(a-b)) / abs(a-b)]
```

The bracket's equal-argument limit is one. Endpoint columns and scattering
coefficients come from Siderust, absorption contributes only to extinction,
and Rayleigh/aerosol phases and photon-direction sign are shared with #190.
There is no second medium or transport equation. Analytical, split-layer,
unchanged GL and independent SciPy comparisons validate this specialization.

Two successive angular doublings must satisfy the requested radiance-change
target. Active constituent phase integrals over the whole sphere must also
resolve unity at the accepted order; values are reported **without
renormalization**. This prevents falsely accepting vanishing samples of an
unresolved narrow HG lobe. HG/double-HG wavelength-independent phase audits are
shared within the bin; wavelength-dependent strict tables are audited per node.
Sharp phases can still exceed the bounded plan. Successive changes and phase
checks are diagnostics, **not rigorous error bounds** or a guarantee for every
unresolved structure. Independent boundary-integral fixtures test sparse bright
pixels, grazing paths, anisotropy, forward/backward HG and isotropic phases.

## Computational bounds and outputs

Both paths preflight the entire configured spectral plan before sampling,
including levels skipped by early convergence and wavelength-rule construction.
The unchanged caps are 250,000,000 conservative work units and 8 MiB temporary
quadrature storage. For L layers, pixel mode charges `4L²+L` layer preparation
plus `sum(6*nside²*(7+L))` per wavelength, six named phase-audit operations per
antipodal pair, shared validation and wavelength GL construction. Checked
arithmetic rejects overflow and oversized plans. Only layer responses, at most
11 phase-audit descriptors and 64 wavelength nodes are buffered; there are no
whole-sky subpixel arrays or general cache/LUT. Caller-owned maps, medium and
output snapshots are outside quadrature storage, as in #190.

`HealpixSettings::default()` refines native NSIDE twice, with relative 1e-4 and
absolute spectral 1e-12 targets. Coarse synthetic NSIDE1 skies require additional
doublings. Default wavelength settings and a one-layer NSIDE128 map cost
243,600,079 work units per bin, including the complete configured plan.
Smooth solver settings require an independently budgeted spectral combination.

`StarlightTransport` retains typed TOA/direct/single native-bin radiances,
settings, wavelength order/changes, grid, map provenance/checksum, actual medium,
phase, geometry and planned work. The applicable optional settings/diagnostics
identify smooth (`convergence`) or pixel (`healpix_convergence`) mode. Pixel
reports include accepted NSIDE, two angular changes, 2π measure and raw phase
integrals. Higher orders and physical uncertainty remain `None`. Cross-bin
covariance, source systematics and model discrepancy are unknown, not zero.
Missing physics includes multiple scattering, polarization, spherical geometry,
refraction, surface reflection, DGL, internal emission and telescope response.

## Offline reproduction

```sh
cargo run -p nsb --release --example starlight_transport --locked
cargo run -p nsb --release --example starlight_transport --locked -- \
  /absolute/starlight_360_570_candidate.json 2459679.9675055384 2459679.966703672
# Separately labelled historical optical-law/bin-center fixture:
# append --pilot-bin-centers
# Isolate wavelength integration under the current optical prescription:
# append --current-bin-centers
```

The frozen invocation pins the unchanged SHA-256 and exits nonzero on any bin
failure without a partial total. See the [validation record](../validation/starlight-transport-267.md)
for independent references, measured results and historical assumptions.
HEALPix geometry follows its [equal-area discretization contract](https://healpix.sourceforge.io/html/intro_Discretisation_Functions_on.htm);
[finite-precision boundary conventions](https://healpix.sourceforge.io/html/intro_Finite_precision_cross_plat.htm)
can differ extremely close to pixel boundaries.
