# Atmospheric transport

Status: Stage 1 audit and implementation contract for [#187](https://github.com/VPRamon/NSB/issues/187)
and [#190](https://github.com/VPRamon/NSB/issues/190), 2026-10-17.
**Only identity and direct transmission are implemented.** Scattering below is a
scientific contract for an internal experimental implementation, not a public API
or a change to default predictions. Instrument response (#189), DGL (#188),
multiple scattering and acceleration (#192) remain separate work.

## Audited versions and implementation status

| Source | Reviewed identity |
| --- | --- |
| NSB latest main | `0fd177079016a3dcb168ecfad7277511b11eb31e` |
| Siderust latest main | `0ad41efb4a23dd39f6aaa50041bc7e64d53bcb85` |
| NSB locked runtime | Siderust 0.14.1, QTTY 0.8.6, AFFN 0.11.0, Optica 0.4.0 |
| Independent evidence | nsb-validation `71bfce532f1911b795fa7658bde1e0edbc1cf775`, Outcome C / admission Level 1 |

Both upstream main branches were fetched for this audit. Siderust main's
`src/atmosphere/` matches the published 0.14.1 sources used by NSB byte-for-byte.
No dependency update is needed for existing transport; future primitives need an
explicit release/compatibility decision. PR #193's historical description and
#190's old placeholder names are not authoritative descriptions of current code.

| Current NSB source (`crates/nsb/src/`) | Implemented contract |
| --- | --- |
| `transport/model.rs`, `mod.rs` | `TransportModel::{Identity,Direct}`, four `AirmassModel` variants; typed energy/photon spectral application |
| `transport/direct.rs` | `DirectTransmission`: `exp(-tau X)`; ingredient/airmass accessors; positive finite wavelength check |
| `transport/extinction.rs` | `ExtinctionIngredients`, `MolecularAbsorption`, `OpticalDepthBreakdown`: Rayleigh + aerosol extinction + optional bundled ozone |
| `transport/geometry.rs` | `DirectPathGeometry`: finite zenith in [0,90] degrees; this acceptance is not a horizon accuracy guarantee |
| `transport/origin.rs` | `RadianceOrigin::{TopOfAtmosphere,AtmosphericEmission,PreScatteredAtmosphere}` |
| `transport/metadata.rs` | `TransportModelMetadata`: model-only facts; atmosphere identity belongs to actual evaluation context |
| `site/atmosphere.rs` | `AtmosphericConditions` stores **observer-local** pressure; Rayleigh helper supplies zero altitude correction |
| `components/starlight/map.rs`, `interval.rs` | `StarlightMap` TOA integrals, spectral coverage, provenance, direct-transmission bounds; no scattered-light evaluator |
| `band.rs` | `NsbEvaluator::integrate_band`: restricted existing component spectra; rejects Starlight in ground totals |

Identity passes the input through exactly and accepts every origin. Direct
spectral apply accepts only `TopOfAtmosphere`: applying it to Airglow or
pre-scattered Jones/KS91 Moonlight is rejected. No `ScatteringGeometry`,
`ScatteredPath` or `ScatteringPathStatus` exists in current NSB. No public phase
wrapper or generic in-scattering solver exists. Preserve these origin safeguards.

Zodiacal retains `ZodiacalExtinction`, Airglow retains its emitting-volume and
Noll attenuation, Moonlight retains embedded Jones/KS91 scattering, and existing
Starlight admission/default behavior stays unchanged. Transport selection must
never rewrite source identity, calibration status or scientific maturity.

The current direct path validates wavelength and constructed geometry, but does
not call `AtmosphericConditions::validate` when given a caller-modified public
conditions struct; it does not validate incident radiance either. Ozone lookup
also clamps its spectral domain. These are existing enforcement limits, not
permission for the new solver to accept invalid states. Stage 2 must validate
its complete request; tightening legacy direct validation requires a separate
compatibility decision. No default numerical path is changed in this audit.

## Siderust primitive audit

Paths in this table are relative to the audited Siderust tree unless qualified.
Typed units do not imply runtime physical-domain validation. The limits column
records actual behavior, not stronger guarantees inferred from type names.

| Primitive and source | Physics / units / convention | Tests inspected and NSB suitability / limits |
| --- | --- | --- |
| `rayleigh_phase`, `atmosphere/rayleigh.rs`; `RayleighPhaseFunction`, `atmosphere/scattering.rs` | `3(1+cos² theta)/(16 pi)`, integral over 4pi = 1; angle `Radians`; return `Quantity<ScatteringFactor>` (dimensionless marker; physically sr^-1) | 0/90-degree values and forward/back symmetry. Analytically normalized; no integral test or runtime angle check. Wavelength argument of trait implementation ignored; ideal scalar, unpolarized, no depolarization correction. Reuse with checked input domain. |
| `rayleigh_optical_depth_bodhaine99`, `atmosphere/rayleigh.rs` | Rational approximation in micrometers; dimensionless `OpticalDepths`; scale `p_ref/1013.25 hPa * exp(-h/H)`; `Kilometers`, default H=8 km | Broad 550-nm Paranal-like range test only; not spectral empirical validation. No checked wavelength/pressure/H domain. Reference-pressure plus height OR local-pressure plus zero, never both local pressure and nonzero height. Optical-depth approximation is not a molecular density profile. |
| `MieParams`, `mie_optical_depth`, `atmosphere/mie.rs` | Extinction `tau0*(lambda/lambda_ref)^alpha`; typed optical depth/nm, dimensionless exponent (negative for usual aerosols) | Reference equality, monotonic wavelength behavior, observatory ordering, literature-range and provenance tests. No SSA, absorption/scattering split, g, phase function or height distribution. Parameters are public and unchecked; preset median is not exposure calibration. |
| `PhaseFunction`, `atmosphere/scattering.rs` | `phase(Nanometers,Radians)->Quantity<ScatteringFactor>` | Infallible trait; no normalization/domain/provenance requirement. Module text allows 1 or 4pi normalization. Not every implementor is a probability density. Solver must use a checked contract. |
| `TabulatedPhaseFunction`, same file | Private Optica `Grid2D<Nanometer,Degree,ScatteringFactor>`; rows angle, columns wavelength; bilinear interpolation in wavelength and **degrees**, not cos(theta) | One interpolation test. Axis/shape/finiteness validation delegated to grid; negative finite values, incomplete angular coverage and unnormalized correction tables remain possible. All out-of-range queries clamp. No exposed domain/strict evaluator. Reuse only with independently retained validated axes/domain until upstream checked API exists. |
| `transmission`, `atmosphere/extinction.rs` | `exp(-tau*X)`; `OpticalDepths`, `Airmasses` -> `Transmittances` | Zero tau and monotonic tau/X tests. No invalid-state guard. Valid nonnegative finite optical paths reuse directly; no source term. |
| `airmass::<F>`, `atmosphere/airmass.rs` | `PlaneParallel` secant; `Young1994`, `Rozenberg1966`, `KrisciunasSchaefer1991`; angle radians -> dimensionless `Airmasses` | Zenith approximately one, secant horizon divergence, finite KS91 horizon. Empirical effective columns are not local slant integrals. Do not apply them inside the layered plane-parallel solver. |
| `AtmosphereProfile`, `atmosphere/profile.rs` | Local surface pressure hPa, altitude km, Rayleigh H km, aerosol parameters; column Rayleigh+Mie and transmission | Presets, wavelength ordering, transmission monotonicity, local-pressure/no-double-reduction regression. Current implementation correctly passes zero altitude to Rayleigh. No spatial layers, TOA, SSA or ozone distribution; preserve API. |
| `ozone::transmission_table`, `transmittance_at`, `atmosphere/ozone.rs` | Bundled wavelength/transmittance spectrum (~300–1000 nm); NSB maps to vertical `-ln(T)` | SHA, monotone wavelengths, [0,1] samples, lookup tests. Endpoint clamping; no site ozone amount or vertical density. Treat as an optional column assumption, not a height-resolved absorber. Strict solver rejects unsupported spectral range. |
| `HealpixGrid::{direction_to_pixel,pixel_center,pixel_center_spherical,pixel_area_sr}`, `healpix/grid.rs` | Frame-tagged directions, RING centers, equal area `4pi/(12 nside²)` sr (raw f64 area at this boundary) | `healpix/tests.rs`: indexing, centers/round trips, validation, map consistency. Reuse for later sky adapter; NESTED lookup unsupported. Pixel mean is not point-source/fiber radiance; no RT angular integrator. |
| `coordinates` AFFN direction aliases; `coordinates/horizontal.rs`, `transform/horizontal.rs`, `transform/frames/to_horizontal.rs` | Frame/center-tagged directions; North-clockwise azimuth, geometric altitude upward. GMST/GAST paths have distinct accuracy assumptions | Convention and transform tests in those modules; AFFN `cartesian/direction.rs` tests normalized directions, dot/angle and degeneracy. Reuse `angle_to` / `dot` after bringing both vectors to the same local frame. Record epoch/transform/refraction choice; frame reinterpretation is not a transform. |
| QTTY `Quantity`, `Per`, radiometry and solid-angle units (0.8.6); Optica `spectrum/integrate.rs` (0.4.0) | Spectral energy/photon radiance, nm, m, sr; `trapz`, `trapz_range`, `trapz_weighted` re-exported from `spectrum/algo.rs`; typed integration on `SampledSpectrum` | Quantity/unit conversion tests, Optica analytic spectrum/integration tests. Existing NSB `band::integrate_linear` uses `trapz_range`. Spectral trapezoids are not an atmospheric path or angular solver; no generic RT quadrature was found. No QTTY/AFFN change justified. |

A repository-wide search of current Siderust `src` found no Henyey–Greenstein
phase function or height-resolved optical-coefficient abstraction. The existing
`AtmosphericConditions::mie_params` comment mentions phase parameters, but the
actual upstream struct contains only extinction parameters.

### Pressure, wavelength and phase invariants

- Local pressure already represents the overlying hydrostatic column. NSB's
  helper and current `AtmosphereProfile` both supply `h=0` to Bodhaine. Use that
  same column to normalize Rayleigh scattering coefficients. Observer altitude
  locates boundaries; it must not reduce that column a second time. At p=0 the
  ideal column is zero for finite valid wavelength/H; current NSB site validation
  requires positive pressure, so experimental vacuum fixtures need an explicit
  zero-coefficient column rather than an invalid site.
- The Bodhaine rational kernel is unchecked outside its intended optical use;
  positive wavelength alone is not a scientific validity domain. Stage 2 starts
  with **360–570 nm**. Broader claims require reference evidence. Rayleigh's
  wavelength-independent angular shape does not extend the column law's domain.
- For angles between photon **propagation** vectors, theta=0 is forward and pi
  is backward. If `n_in` points toward the incident sky and `n_view` points from
  observer toward the scattering location, `k_in=-n_in`, `k_out=-n_view`, hence
  `cos(theta)=n_in dot n_view`. Reversing just one sign hides behind Rayleigh
  symmetry but reverses an aerosol forward peak.
- Solver convention: `integral P dOmega = 1`. Rayleigh integrates analytically:
  `2pi * 3/(16pi) * integral[-1,1](1+mu²)dmu = 1`.
  `P(0)=P(pi)=3/(8pi)`, `P(pi/2)=3/(16pi)`.
  External 4pi-normalized values convert by **division by 4pi**.
- An HG surrogate, once upstream, uses
  `P=(1-g²)/(4pi*(1+g²-2g cos(theta))^(3/2))`, finite `-1<g<1`.
  Positive g is forward, g=0 isotropic, and g=±1 are excluded delta limits.
  Record g's wavelength assumption; HG is not a Mie microphysics calculation.
- Validate nonnegative finite tabulated values, full [0,pi] support and the
  integral of the **actual interpolant** at each wavelength. Linear wavelength
  interpolation preserves normalized slices. Reject out-of-domain queries
  before legacy clamping; never silently normalize an arbitrary correction table.
  Check measured angular normalization error against declared tolerance.

## Ownership and upstream prerequisites

| Owner | Responsibility / follow-up |
| --- | --- |
| Siderust | Generic checked extinction/scattering/absorption coefficients, finite layers and column integrals: [Siderust #106](https://github.com/Siderust/siderust/issues/106). Additive beside `AtmosphereProfile`. |
| Siderust | HG surrogate and strict, domain-aware normalized table path while preserving legacy clamping: [Siderust #107](https://github.com/Siderust/siderust/issues/107). |
| NSB site | Select actual atmospheric parameters/profile; preserve local-pressure semantics and site calibration identity. No invented layer distribution from existing site fields. |
| NSB transport | Origin policy, celestial illumination boundary, reference angular/path integration and contribution composition, diagnostics and maturity. |
| NSB Starlight | Read exact stored spectral bins, capabilities, source identity, uncertainty and coverage; later adapt Galactic RING pixels to local directions. |
| QTTY / AFFN / Optica | Reuse quantities, frame geometry, tables and spectral integration; no new generic framework or dependency changes in Stage 1. |

No executable upstream change is needed to complete the audit. Stage 2 must
resolve/release the required upstream subset, verify selected-version
compatibility and retain Cargo.lock deliberately; it must not grow a second
atmosphere library in NSB. The proposed names in upstream issues are design
sketches, not symbols available in Siderust 0.14.1.

## Vertical atmosphere contract for Stage 2

Use a scalar, elastic, horizontally uniform **plane-parallel** slab. The initial
reference representation is contiguous, piecewise-constant layers, with exact
vertical optical-depth sums. Let absolute MSL height `z` increase upward; ground
and observer initially coincide at `z0`, with finite top `zt > z0`. Keep observer,
ground and TOA explicitly identified even in this restricted case. Interior
observers, surface reflection/emission and spherical/refraction paths are
unsupported initially and must produce an error when requested.

At each supported wavelength and layer, use inverse-meter coefficients:

```text
beta_ext = beta_R + beta_aer_ext + beta_mol_abs
beta_sca = beta_R + beta_aer_sca
beta_abs = beta_aer_abs + beta_mol_abs
beta_aer_sca = omega_aer * beta_aer_ext
beta_aer_abs = (1 - omega_aer) * beta_aer_ext
beta_ext = beta_sca + beta_abs
```

All coefficients are finite and nonnegative, and `0 <= beta_sca <= beta_ext`.
Rayleigh is nonabsorbing in this model. SSA `omega_aer` is finite in [0,1]; the
ratio `beta_sca/beta_ext` is **undefined** at zero extinction. A vacuum layer has
both coefficients zero and no required phase/SSA evaluation. Aerosol extinction
never implies SSA=1. No aerosol means phase configuration is unnecessary.
Molecular absorption contributes extinction but never a scattering source.

Layer edges are finite, strictly increasing, contiguous and cover [z0,zt];
upper-edge ownership is next-layer except the final closed top. Outside-domain
queries fail. Validate coefficients and optical-depth sums for overflow too.
For constituent c:

```text
tau_c(lambda; a,b) = integral[a,b] beta_c(lambda,z) dz
                 = sum_layers beta_c * intersected_layer_thickness
```

The above-observer Rayleigh/aerosol column must equal the selected NSB direct
column under matched ingredients. Existing column data do not determine
vertical shape: require explicit layer fractions (each constituent's fractions
sum to one), or a separately declared shape. A future exponential adapter uses
`f(z)=exp(-(z-z0)/H)/(H*(1-exp(-(zt-z0)/H)))` over [z0,zt],
`beta_c=tau_c*f`; do not silently truncate and lose column mass. Aerosol tau0
already describes the over-site extinction column. Ozone needs its own declared
distribution; reject bundled-column ozone in a scattering profile until one is
explicitly supplied. The initial reference may select molecular absorption zero.

The homogeneous one-layer test fixture and explicit two-layer mixtures are
required; pressure, aerosol H or surface properties must not be inferred from a
site name. Different vertical mixtures can change scattering with equal columns.

For output `mu_o=cos(zenith)>0`, `ds=dz/mu_o` and exact partial-column attenuation
uses this same medium on both incoming and outgoing segments. Output zenith is
initially limited to [0,80] degrees as a conservative **implementation policy**,
not an empirical accuracy claim. Incoming directions cover `0<mu_i<=1`; no
silent low-altitude cut. Quadrature must converge near mu_i=0 using attenuation
limits, not evaluate a secant at zero. The mathematical plane-parallel horizon
limit is not a spherical-atmosphere prediction. Do not insert Young/KS91 effective
airmass into local segments; preserve them only in the existing direct model.

## Scattering and contribution contract

The illumination source owns TOA radiance `I0(lambda,n)` and its coverage. It is
not an already attenuated ground field. Initial bottom boundary is black,
nonemitting; no incoming radiation from below, no internal emission, no solar
beam unless explicitly supplied as a source. For incoming sky directions:

```text
I_inc(lambda,z,n_i) = I0(lambda,n_i) * exp(-tau_ext(lambda; z,zt)/mu_i)
j1(lambda,z,k_out) = sum[c=Rayleigh,aerosol] beta_c_sca(lambda,z)
                    * integral[incoming hemisphere]
                      P_c(lambda,acos(k_in dot k_out)) * I_inc dOmega_i
L1(lambda,n_view) = integral[z0,zt] j1(lambda,z,-n_view)
                    * exp(-tau_ext(lambda; z0,z)/mu_o) dz/mu_o
L0(lambda,n_view) = I0(lambda,n_view) * exp(-tau_ext(lambda; z0,zt)/mu_o)
```

Keeping constituent source terms separate avoids an extinction-weighted phase:
if a mixed phase is formed, weight it by **scattering**, not extinction.
Both incident and exit attenuation use total extinction including absorption.
`L1` already includes those attenuation factors and must not be fed through a
second direct transmission. The two terms share atmosphere/geometry provenance;
matching legacy plane-parallel direct requires the same column and secant.
Adding legacy Young direct to this first-order result without declaring the
geometry discrepancy is unsupported.

Physical dimensions are `P: sr^-1`, `dOmega: sr`, `beta: m^-1`,
`I_lambda: photons/(cm² ns sr nm)` (or W/(m² sr nm)),
`j_lambda: I_lambda/m`, `ds: m`; integration returns the same spectral radiance.
Optical-depth exponent is dimensionless. Integrated-band radiance has no `/nm`.
Use existing `crate::units` quantities, not untyped public arrays. The upstream
`ScatteringFactor` is dimensionless, so convert its value at a named, checked
seam into a `Quantity<Per<ScatteringFactor,Steradian>>` convention before angular
weighting. Coefficients can use `Quantity<Per<OpticalDepth,Meter>>`; these are
compositions of existing QTTY units, not new physical units. Numerical kernels
may unpack f64 once at documented boundaries and restore typed results; compile
checks must prevent mixing band and spectral inputs or m and km.

### Internal experimental API shape (not exported)

Keep `TransportModel`, its defaults and origin rules unchanged. Implement a
private reference entry point under transport, conceptually:

```text
single_scattering(lambda: Nanometers,
                  toa: checked incident spectral samples + Steradians weights,
                  view: same-frame direction,
                  medium: checked Siderust layers,
                  settings: angular/path tolerances)
  -> Result<{direct: PhotonSpectralRadiance,
             single: PhotonSpectralRadiance,
             higher_order: Option<PhotonSpectralRadiance>, diagnostics}>
```

This is a type/ownership sketch, not compilable Rust or a promise of new public
symbols. Initial implementation can use photon radiance only; energy support
must use the same elastic operator if added. Samples include a separately
queryable TOA radiance along the view for direct light; angular nodes alone must
not invent a direct point value. NSB owns the source/quadrature adapter, path
integration and result composition; Siderust owns layer optics and phase
primitives. Start with analytic/synthetic angular fields and fixed deterministic
quadrature. No full HEALPix integration in #190.

Future higher orders replace incident zero-order radiation in the source term
with the previous scattering-order field. The boundary/medium and contribution
semantics stay the same. `higher_order=None` means **uncomputed**, not physical
zero. An explicitly labelled `direct + single` partial result is allowed;
`total_ground` is unavailable until all requested contributions are computed.
Airglow and pre-scattered Moonlight need different volume/boundary contracts.

Invalid atmosphere, negative/nonfinite radiance, unsupported origin/domain,
unnormalized phase, incomplete source coverage or incompatible spectral input
must return explicit `NsbError::OutOfRange` / `Unsupported` (or a contextual
upstream error). A convergence failure returns an error/failed diagnostic and
must not be labelled validated. No silent clamping, missing-as-zero or extrapolation.

## Spectral Starlight integration boundary

Merged #254/#257/#258 do not provide a continuous Starlight SED.
`StarlightMap::{band_nm,bin_edges_nm,sha256,provenance,grid,pixel_direction,
pixel_solid_angle}` supplies coverage/identity and later angular geometry.
`top_of_atmosphere_pixel` returns native photon integral and optional statistical
sigma. `integrate_band` returns **exact stored-bin sums** and rejects partial
bins/gaps. Multi-bin sigma needs packed covariance or the recorded native-band
sigma. `direct_pixel_bounds` is an attenuation envelope, not an in-scattering
operator. `NsbEvaluator::integrate_band` must continue rejecting TOA Starlight in
ground totals until separately validated transport is explicitly selected.

Frozen XP: Galactic RING NSIDE=128, seven adjacent 30-nm bins with edges
`[360,390,420,450,480,510,540,570]` vacuum nm, checksum
`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
Do not modify/regenerate it. The 205,143,002 admitted XP sources do not constitute
a complete stellar population or an instrument/fiber input. Legacy 300–650 nm
behavior is a separate admission contract.

For a nonnegative elastic transfer kernel K(lambda,n), a stored bin provides
`F_b(n)=integral_b I_lambda dLambda`, not `I_lambda(center)`. Exact transported
bin integrals require the SED or a wavelength-constant kernel. With only F_b,
`min_b K * F_b <= integral_b K I_lambda dLambda <= max_b K * F_b` per source
sample; positive angular weights preserve these bounds. Extrema must be
certified analytically or by a refined search with an explicit residual, not
called rigorous merely because a finite wavelength mesh was sampled.

An opt-in flat-photon-SED model uses `I_lambda=F_b/DeltaLambda` and integrates
wavelength-dependent K over the bin; report the assumed shape and the separate
within-bin envelope. A center-kernel estimate may be retained only as a labelled
pilot compatibility approximation, with refinement/shape sensitivity. Neither
creates spectral information. Partial-bin exact requests fail. Later sky
integration preserves pixel area, horizon treatment, ordering and source frame;
validate angular convergence independently of fixed map spatial resolution.

Missing cross-bin/pixel covariance must not be invented. The covariance-aware
native sigma cannot in general be reweighted by a wavelength-dependent kernel.
Use supplied covariance where available, otherwise return unavailable propagated
sigma or a labelled conservative `sum |w_i| sigma_i` bound. Preserve unknown Gaia
systematic calibration, missing populations and model discrepancy separately.

## Fixed independent evidence from #263

Authority: [final report](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/reports/starlight-empirical-validation-final/report.md),
[structured evidence](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/reports/starlight-empirical-validation-final/evidence-v1/evidence.json),
[reproduction](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/reports/starlight-empirical-validation-final/reproduction.md),
[protocol](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/configs/starlight-closure-reference-v1.json),
[producer](https://github.com/VPRamon/nsb-validation/blob/a483f81e77034d140bc6f2a889fb49f6b418aab7/scripts/compare_xp_disort.py).
The actual saved `disort-v4/reference-result.json` and prior
`scattering-129304-v2/scattering-result.json` were inspected for geometry,
producer/input pins and numerical settings; reproduction paths remain in that
guide. No holdout data is needed or authorized.

| Reference condition | Fixed value / meaning |
| --- | --- |
| Source | Frozen XP checksum above; seven stored bin integrals; **bin-center atmospheric kernels** at 375,405,435,465,495,525,555 nm; no invented SED |
| Geometry | DESI exposure 129304, b0, FIBERMAP row 0, mid-exposure `MJD-OBS + EXPTIME/(2*86400)` from pinned frame; site lon -111.59989°, lat 31.96403°, MSL 2097 m; pressure=0 in Astropy transform disables refraction |
| Output / input sky | Saved output altitude 76.92569110795976°, North-clockwise azimuth 271.8975544363482°; Galactic RING pixel centers transformed at that epoch, source altitude strictly >0; NSIDE=128, weight `4pi/(12*128²)` |
| Rayleigh | Local pressure **779.5 hPa** (declared approximation, not measured); `tau_R=(779.5/1013.25)*0.008569*x^-4*(1+0.0113*x^-2+0.00013*x^-4)`, x=lambda in micrometers. This is the **historical nsb2 approximation, not NSB's Bodhaine rational kernel**. Use a labelled reference fixture, never retune production physics to hit the number. |
| Aerosol | `tau_ext=0.03*(lambda/550 nm)^-1.38`; HG g=0.65; nominal aerosol SSA=1; pure Rayleigh scattering; molecular absorption=0 |
| Layers / boundary | One constant optical mixture in plane-parallel optical-depth coordinate; black nonemitting ground, no internal emission/direct solar beam, no incoming lower-boundary radiation. No physical scale height was measured or prescribed. |
| nsb2 first order | Commit `bc9320db03fbff69997ce366c6e76a37339c5d00`; secant geometry, exact homogeneous-column scattering kernel; NSIDE 16/32/64/128 study; nominal **0.0025991471011773495** photons/(cm² ns sr) |
| External solver | PythonicDISORT **1.8** (PyPI runtime); producer `a483f81e77034d140bc6f2a889fb49f6b418aab7`; separate upstream benchmark `febb19f290a2ad860f74922d58d2f719e2fde771`. Python 3.12.13, numpy 2.5.4, scipy 1.18.1, astropy 8.0.2, healpy 1.20.1; runtime source hashes and boundary shape-classification patch recorded in report manifest. Do not port the patch into NSB. |
| Phase convention | NSB uses integral P=1; DISORT uses unweighted Legendre moments with beta_0=1, Rayleigh beta_2=0.1 and HG beta_l=g^l; `P=1/(4pi)*sum_l (2l+1) beta_l Legendre_l(cos theta)`. Mixture moments weighted by scattering. |
| Boundary projection | Positive-mu GL cells with edges cumulative weights; accumulate pixel solid-angle integrals of cos(m delta-az) about output azimuth; divide by `2pi*GL_weight`, double m>0. Sine contribution cancels by reflection symmetry at output azimuth. Sparse sky is not sampled at GL points. Saved vertical-flux projection residual must be checked. |
| Numerical settings | (streams,Fourier) = (16,16),(32,32),(64,16),(64,32),(128,32),(128,64),(256,32); hemisphere uses streams/2 GL nodes; cubic-spline output interpolation in mu at ground |
| First-order extraction | Hold extinction fixed, scale scattering albedo by epsilon=1e-4 and epsilon/2; remove exact zero-scatter projected-boundary attenuation; Richardson `4*L(epsilon/2)/epsilon - L(epsilon)/epsilon` |
| Nominal full scattering | SSA multiplied by `1-1e-7` for conservative regularization; sensitivity at `1-1e-6`; direct boundary removed; converged 256-stream scattered result **0.0033296021852152395** photons/(cm² ns sr), including first and higher orders |
| Independent first order | **0.0025990756849478282**, relative difference from nsb2 0.00274768%; existing gates: first-order <1%, multiple 64/128 change <0.5%, Richardson change <0.1%; observed multiple 128/256 change 0.00600502% |
| Input checksums | frame `27ab272d2a8ac0bcd2978149f51dcccce3732d853be54654545484e87307ea4d`; prior scattering result `b2580097503445d445504185f160aa03c44d415f60da78e0b8a842b9172c0172`; protocol `89bf996cbf653d819310b4a4a5eb3c17ffbe0ca630014b8bfa3233b2a88b0b4a` |

The producer's cell projection exactly preserves integrated cosine angular
moments; vertical flux, which also depends on mu, is only approximated at GL
nodes and has a separately recorded residual. Do not interpret the protocol's
short “flux conserving” label as exact flux conservation at finite resolution.

The ~28.10% full-scattered excess is case-specific, **not** a correction factor.
Aerosol SSA=0.9/0, tau550=0/0.1, layer relocation and white-surface cases are
structural scenarios, not measured parameters or confidence intervals. Missing
physics includes polarization/depolarization, curvature/refraction, actual
vertical/weather distributions, molecular absorption in this pilot, incomplete
stellar populations, variability and source systematics. No direct fiber
radiance, DGL or empirical accuracy admission follows. Outcome C / Level 1
remains the only supported decision; Level 2/3 and holdout evaluation remain
unsupported. Full XP replay is a later all-sky adapter acceptance, not required
to implement synthetic-field #190 or a reason to implement multiple scattering.

## Stage 2 reference method and validation gates

Start with deterministic positive angular quadrature (GL in mu, periodic azimuth)
and layer-split path quadrature with explicit tolerances and convergence records.
Compare both independently refined angular and path resolutions; no random seed,
network, LUT or hidden cache dependencies. A homogeneous layer admits an analytic
path integral and is an independent oracle for the numerical path integration.
For a constant incident angular patch of solid angle DeltaOmega, homogeneous
optical depth tau, fixed mixed phase P, SSA omega, and incoming/output positive
mu_i,mu_o:

```text
L1 = I0 * P * DeltaOmega * omega/mu_o
     * (exp(-tau/mu_i) - exp(-tau/mu_o)) / (1/mu_o - 1/mu_i)
mu_i = mu_o = mu: L1 = I0 * P * DeltaOmega * omega * tau/mu * exp(-tau/mu)
```

Use the continuous equal-mu limit and stable divided differences / `expm1` near
it. For a collimated test, `I0*DeltaOmega` is the fixed angular integral; this
is not a finite-width patch's exact angular integration. Multiple constituents
use scattering-weighted P. This oracle comes from the exponential path integral,
not from copying a second implementation of a solver loop.

| Required check | Class / acceptance |
| --- | --- |
| Vacuum / zero tau | Analytic: direct equals TOA exactly, single zero; no 0/0 phase or SSA evaluation |
| Zero scattering / pure absorption | Analytic: single zero, direct Beer–Lambert; arbitrary absorption cannot generate light |
| Pure Rayleigh / aerosol SSA | Analytic + physical: Rayleigh endpoints and symmetry; SSA=0 and 1 limits; at fixed extinction first-order is linear in aerosol SSA, with Rayleigh term fixed |
| Phase normalization | Analytic + quadrature: Rayleigh and HG integrals=1; HG first moment=g; tabulated actual interpolant; independent angle refinement, including forward peak |
| Optical-depth consistency | Analytic: exact layer sums equal configured direct columns; split/merge homogeneous layers invariant; local-pressure vs reference-pressure equivalence, no double altitude reduction |
| Homogeneous path / small tau | Analytic: formula above including equal mu and tau->0; L1 linear leading term, zero intercept; direct and first order remain individually identifiable |
| Angular symmetry | Analytic: isotropic illumination rotational invariance around vertical; Rayleigh theta/pi-theta symmetry; HG forward/back sign test; no unsupported isotropy assumption for XP |
| Energy budget | Physical + analytic: integrate outgoing **flux** (`I*mu*dOmega`) at top and bottom plus absorption. Full conservative solution equals incident flux; zero+first order alone leaves a higher-order remainder and must never be forced to conserve exactly. In thin smooth fields verify first-order cancellation and higher-order remainder scaling; do not claim a uniform O(tau²) bound at grazing incidence. |
| Units | Compile/quantity conversion checks: spectral vs band, photon vs energy, m vs km, sr weighting, nm integration. Same elastic operator and energy/photon conversion must commute at fixed wavelength. |
| Convergence | Numerical: refine path/angular independently, then jointly; target relative change <=1e-4 for nonzero synthetic cases, absolute <=1e-12 photons/(cm² ns sr nm) near zero, both declared in fixture. Require analytic error within that budget; successive-resolution agreement alone is not an error proof. Record failed convergence. |
| Finite/nonnegative/domain | Physical-domain tests: NaN/inf, negative radiance/coefficient, SSA outside [0,1], reversed/gapped layers, unsupported wavelength/zenith, table edge/beyond edge, phase normalization failure; no hidden clamping. Property tests over finite valid coefficients/geometries and SSA endpoints. |
| Direct/first-order separation | Analytic/regression: direct matches existing PlaneParallel with same column; single never receives final direct attenuation again; no TOA mutation, no default evaluator migration; reject in-atmosphere/pre-scattered origins |
| External matched synthetic cases | Cross-implementation: independent scalar reference with same tau/SSA/P/boundaries, and documented unit/phase conversion; fixtures with known input/output pins. No empirical-accuracy claim. |
| XP frozen pilot (later adapter) | Scientific reference regression: verify all pins/conditions above and existing <1% first-order gate without fitting; separately quantify Bodhaine-vs-pilot column and bin-shape differences. Multiple-scattered number is context, not a #190 required solver output. |
| Offline repeatability | Regression: identical inputs/settings yield deterministic output/order and provenance without network; bounded work, explicit settings and diagnostics |

`crates/nsb/tests/atmospheric_transport.rs` already covers identity, Beer–Lambert,
monotonicity, ingredient/airmass selection, origin rejection, invalid geometry /
wavelength, model metadata, local-pressure parity and a labelled nsb2 direct
comparison. Preserve those tests. Numerical agreement is evidence for a stated
model and implementation, not proof of empirical astrophysical accuracy.

When executable code changes, run fmt, workspace all-target/all-feature Clippy,
workspace all-target tests and Rust 1.89 all-target/all-feature check with the
locked dependency graph, plus relevant transport tests and upstream checks.
New/changed production code needs independent reference, boundary and property
tests and >=95% changed line coverage under existing policy. Stage 1 changes are
documentation/issue-only: executable coverage is not applicable.

## Provenance, uncertainty and maturity

Reuse `TransportModelMetadata` for immutable model facts, `SiteProfile` /
`AtmosphericConditions` for actual atmospheric context, `StarlightProvenance`
for source identity and existing maturity/calibration contracts. Do not add a
free atmosphere-name string disconnected from the evaluated medium. A small
internal evaluation record can carry the missing numerical and contribution
facts; no general uncertainty framework is needed.

Record model ID/version, medium identity and serialized parameters/layer edges,
local-pressure convention, extinction/scattering/absorption constituents, aerosol
SSA/g/phase identity and spectral assumptions, phase normalization, TOA source
checksum/coverage/origin/maturity, surface boundary, geometry/frame/epoch,
solver identity/tolerances/resolutions, convergence results, within-bin model,
domain/extrapolation policy, and missing physics. Higher-order status and source
population completeness must be explicit. Preserve model/source maturity;
new transport remains **experimental** pending separate admission.

| Uncertainty category | Representation / policy |
| --- | --- |
| Numerical approximation | Measured convergence estimates and analytic residuals; path/angular/spectral/projection errors distinguished; a resolution difference is not automatically a rigorous bound |
| Source statistics | Supplied covariance or explicit conservative bound; missing covariance stays unavailable |
| Source systematics | Gaia common calibration etc.: known value or **unknown**, never zero by default |
| Atmospheric parameters | Supplied distribution/uncertainty or labelled sensitivity scenarios; no conversion of scenarios to confidence intervals |
| Model discrepancy | Scalar/plane-parallel/HG/first-order limitations: unquantified unless separately established |
| Missing populations/physics | Explicit unavailable contribution, not a sigma=0 or a source completeness correction |

## Implementation sequence and open scientific decisions

1. Resolve Siderust #106/#107's required checked primitives and release decision.
2. Implement #190's internal opt-in reference with synthetic spectral illumination,
   homogeneous/two-layer optics, normalized Rayleigh/HG, separate direct/single
   and the validation gates above. Higher orders remain uncomputed.
3. Add spectral Starlight sky adapter with coverage/within-bin treatment and
   reproduce the pinned pilot under explicitly matched legacy optics.
4. Validate a separate multiple-scattering reference before total-scattered
   claims. Only then consider #192 acceleration with reference-equivalence budgets.
5. Reassess maturity/admission explicitly; never silently enable default transport.

No unresolved convention blocks the reference implementation. Physical questions
remain for later admission: measured vertical aerosol/ozone profiles, wavelength-
dependent SSA/g or empirical phase tables, spherical/polarized transport near the
horizon, surface reflection, source completeness/systematics and observational
calibration. These require evidence, not architecture changes or fitted factors.

## Scientific references

- Bodhaine et al. (1999), Rayleigh optical depth; current rational implementation
  and its local-pressure semantics are audited above.
- Patat et al. (2011), Paranal aerosol extinction; not an aerosol SSA measurement.
- Hansen & Travis (1974), phase conventions; Noll et al. (2012), component context.
- [#263](https://github.com/VPRamon/NSB/issues/263) and the pinned validation records
  above distinguish numerical physical validation from empirical admission.
