# Experimental spectral Starlight single scattering (#267)

`nsb::transport::single::starlight` connects the checksum-validated
`StarlightMap` to the existing #190 reference operator. It is opt-in Rust API;
it does not change `NsbEvaluator`, Python bindings, CLI predictions, production
admission, legacy 300–650 nm products, or scientific input assets.

## Input and geometry

Construct `StarlightSky::new(map, geometry, [low, high],
WithinBinSpectrum::FlatPhotonDensity)` for **one complete stored bin**. Iterate
`map.bin_edges_nm().windows(2)` and sum successful results to transport a union
of native bins. Do not report a union if any constituent evaluation fails.
Partial bins, missing edges, legacy integrated-only data and extrapolation fail.
All requested edges must be inside both the reference 360–570 nm interval and
the actual atmosphere's wavelength domain. The existing JSON loader enforces
SHA-256, supported schema, complete pixels, finite nonnegative bin radiance,
TOA origin, vacuum wavelengths and source declarations. It supports generic
variable/nonuniform bins and the unchanged frozen seven-bin Gaia XP product.
NESTED spectral lookup remains rejected by the existing loader until Siderust
supports it; RING storage/indexing is preserved without resampling.

`StarlightGeometry` contains geodetic ECEF site and **explicit TT and UT1 Julian
dates for the same physical instant**, supplied by the caller. No time-scale
approximation or network EOP lookup is introduced. Horizontal directions use
north-clockwise azimuth and geometric altitude, without refraction. Site height
must equal the medium's first absolute layer edge. The plane-parallel domain,
black nonemitting ground and output altitude 10–90 degrees are inherited from
#190. Siderust uses IAU 2006 GAST for Horizontal ↔ true-equatorial-of-date and
its frame providers through ICRS to Galactic. Three upstream-transformed local
basis vectors retain this fixed rotation, avoiding repeated nutation calculations
per pixel query. Cartesian rotation handles the Galactic longitude seam and
poles without a custom celestial-coordinate formula. Tests cover three sites,
two epochs, seam/poles and both sides of the geometric horizon; round-trip
Cartesian residuals are checked below 1e-9 (far below a HEALPix pixel width).

The exact direct-view direction is looked up separately by #190. Every lookup
returns the **containing HEALPix pixel's spatial average**, not interpolated
radiance, a resolved star, subpixel photometry, telescope fiber or PSF model.

## Spectral model and numerical integration

The native quantity is photons/(cm² ns sr); #190 needs photons/(cm² ns sr nm).
`FlatPhotonDensity` explicitly sets `I_lambda = I_bin / (high-low)` independently
in each bin. It is a model assumption, not recovery of a measured continuous SED.
QTTY division is typed; an explicit same-scale marker bridge converts the
composite quotient to NSB's named spectral-radiance unit.

`starlight_single_scattering` uses the existing positive Gauss–Legendre rule on
the **entire bin**, doubling wavelength order independently of angular/path
refinement. Both integrated direct and single terms must pass the requested
absolute/relative wavelength-change tolerances. Every wavelength uses #190's
independent angular-only/path-only/joint convergence checks and exact layered
Siderust optical depths, absorption/scattering separation and normalized phases.
No second extinction or scattering equation is applied. No midpoint-kernel
compatibility mode is silently substituted for full-bin quadrature.

The conservative **whole spectral refinement plan** must fit 250,000,000 work
units before any transport call. It charges each configured scalar solver plan
and bounded wavelength GL construction, including refinements skipped on early
acceptance. Each sequential scalar call retains the 8 MiB quadrature-buffer cap.
Only at most 64 wavelength diagnostics are retained.
Use explicitly budgeted scalar settings, for example
`SolverSettings { refinements: 2, ..SolverSettings::default() }` for the supplied
fixtures. Combining both default settings objects exceeds the spectral work cap
and fails explicitly; no refinement budget is silently reduced. Borrowed map data and the
fixed frame rotation avoid new sky buffers. Actual input-map memory, upstream
model snapshots, provenance strings and caller-owned results are outside the
solver's quadrature-buffer accounting, as in #190.

**Pixel discontinuities are a numerical limitation.** Successive smooth-function
quadrature changes are resolution diagnostics, not certified pixel-integral
error bounds. They cannot certify arbitrary unresolved sparse structure or
boundary aliasing. `numerical_limitations` carries this warning in every result.
The deterministic anisotropic NSIDE 1/2/4 fixture fails convergence with HG g=0.85
and the bounded example settings; it must keep failing rather than relaxing
thresholds. #192 needs a conservative HEALPix boundary integration/projection
compatible with the authoritative kernel, and an independently validated bounded
cost model. This PR does not implement that acceleration or claim general
real-map angular accuracy.

## Output and uncertainty

`StarlightTransport` separates typed TOA, direct and once-scattered native-bin
radiance. Higher orders and propagated physical uncertainty are `None`, never
zero estimates. It retains bin edges, explicit spectral assumption, spectral
settings/order/change, per-wavelength angular/path diagnostics, grid, complete
public map provenance/checksum/product ID, actual medium/phase, site/epochs,
view, solver settings and the conservative planned work count.

Wavelength/angular/path changes are not Gaia calibration, cross-bin covariance,
missing-population, atmospheric-parameter or model-discrepancy uncertainties.
No unmeasured uncertainty is manufactured or propagated. Model identity is
`StarlightTransport::MODEL`; missing physics includes higher scattering orders,
polarization, spherical geometry, refraction, internal emission, surface
reflection, DGL and telescope response. No empirical admission is implied.

## Runnable offline example

```sh
cargo run -p nsb --release --example starlight_transport --locked
# Optional fixed-map diagnostic; exact pilot replay is NOT claimed:
cargo run -p nsb --release --example starlight_transport --locked -- \
  /absolute/starlight_360_570_candidate.json 2459679.9675055384 2459679.966703672
```

The second invocation pins the frozen map SHA-256 and requires explicit TT/UT1.
It reports each bin's success or convergence failure and exits nonzero if any
bin fails; it never prints a partial integrated pilot total. See the
[validation record](../validation/starlight-transport-267.md) for assumptions,
measurements and the remaining historical-pilot differences.
