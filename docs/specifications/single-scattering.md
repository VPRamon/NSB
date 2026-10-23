# Experimental single-scattering reference (#190)

`nsb::transport::single::single_scattering` is an opt-in scalar, elastic,
plane-parallel reference. `cargo run -p nsb --example single_scattering` evaluates
an anisotropic wavelength-resolved synthetic sky. Existing evaluator and component
calculations do not call it. No change to default predictions or empirical admission is implied.

The observer is at the first absolute-MSL atmospheric edge; the last edge is a
finite TOA. The lower boundary is black/nonemitting. Supported wavelengths are
360–570 nm and output geometric zenith is 0–80°. Incident quadrature covers all
positive zenith cosines, with no positive altitude cutoff. Local Horizontal
source/view directions must share a site, frame convention and epoch; synthetic
fixtures are epoch independent. Unsupported geometries have no API switches
that could silently fall back to this model.

The solver uses upstream `LayeredAtmosphere`, `AtmosphericOptics` and exact
`vertical_optical_depth` for both incoming and outgoing extinction. Rayleigh,
aerosol scattering, aerosol absorption and explicit molecular absorption remain
separate upstream constituents. Local pressure/profile distribution is upstream;
NSB infers no vertical scale heights. Molecular/aerosol absorption never emits.

For each wavelength, incoming radiance at z is `I_TOA exp(-tau(z,top)/mu_in)`.
The source is the angular integral of that radiance times each constituent's
normalized phase density and scattering coefficient. Integrating the source
times `exp(-tau(ground,z)/mu_out) dz/mu_out` yields single-scattered radiance.
Direct radiance is queried at the exact viewing direction and multiplied by
`exp(-tau(ground,top)/mu_out)`. There is no second extinction of the single term.
All results use photons/(cm² ns sr nm); internal scalar conversions are confined
to the numerical kernel. The named phase/solid-angle seam interprets Optica's
ScatteringFactor as sr⁻¹ and multiplies typed steradians.

Both photon propagation vectors are negatives of sky-pointing directions, so
their dot product sets the scattering cosine. Positive HG g is forward peaked.
Optica supplies Rayleigh/HG/double-HG. Siderust's normalized phase table is queried
fallibly, including spectral endpoints even in vacuum. No table clamping or
renormalization occurs.

The result keeps direct and single separately, with `higher_orders=None`.
It owns the actual upstream medium/phase snapshots, source coverage/identity,
wavelength, local view, requested tolerances and measured convergence settings.
The model identity is `nsb-plane-parallel-scalar-single-v1-experimental`.
Source identities should carry the source version/checksum and geometry/epoch
when applicable. Source statistical/systematic uncertainties, atmospheric parameter
uncertainty, model discrepancy and missing populations are **unknown**, not zero.
Numerical resolution changes do not estimate these physical uncertainties.

## Computational admission and performance

Before any source metadata/radiance query, phase lookup, optical-depth integration
or numerical-array allocation, a private checked-integer estimate admits the
**entire configured refinement plan**. Limits are **250,000,000 work units** and
**8 MiB of peak quadrature buffers**. Exceeding either returns `OutOfRange` with
layer count, settings, estimated work/bytes and both maxima; integer overflow is
an explicit error. There is no partial result or machine-dependent timing cutoff.

For one integration with L layers, mu/azimuth/path orders M/A/Z and N=M*A:

- Angular illumination samples: N.
- Vertical source terms: L*Z*N (each includes incoming attenuation).
- Atmospheric layer visits/comparisons: 2*L²*Z + L². Two exact partial-column
  queries per path point traverse layers; coefficient lookup is conservatively
  charged L comparisons per layer. The direct column adds L once per request.
- Legendre recurrence steps: 64*(M*ceil(M/2) + Z*ceil(Z/2)), charging the actual
  bounded Newton iteration ceiling for both freshly allocated rules.
- Peak buffers: `size_of(Sample)*N + 16*max(M,Z)` bytes on supported targets.
  Angular samples coexist with either the mu rule or the path rule, sequentially.

The work limit applies to the sum of the first four counts. Starting with one
coarse integration, each permitted refinement charges angular-only (2M,2A,Z),
path-only (M,A,2Z) and joint (2M,2A,2Z). The joint result is reused as the next
coarse result without another evaluation. All multiplication/addition is checked.
Admission assumes every layer scatters and every refinement is needed, even for
vacuum, absorption or an early-converging request. Individual order bounds remain
initial M>=2, A>=4, Z>=2, with M<=512, A<=1024, Z<=512 **after** all permitted doublings (1–5).

The kernel remains O(N + L*Z*N + L²*Z + M² + Z²) per integration with bounded
Newton iterations. Repeated wavelength-dependent column queries and repeated
sample allocations remain deliberate reference-solver costs. Accumulation stays
in the existing deterministic order; grazing directions retain exponential
attenuation. No caches, parallelization, optical-depth replacement or #192
acceleration are introduced.

These units are conservative algorithmic counts, not CPU instructions or a
universal wall-time guarantee. Caller-provided source callbacks/opaque phase data,
allocator overhead and owned input/result provenance storage are additional;
the byte limit bounds solver quadrature buffers. Input/result storage scales
with the supplied medium/phase rather than the refinement sequence.

The cap admits all existing reference fixtures, including three-layer defaults
(214,266,360 worst-case units), while rejecting four-layer default plans
(281,384,148). Larger columns can request fewer refinements: 16 layers with two
refinements use 17,556,240 units. This intentional preflight policy cannot credit
a request with early convergence before evaluating it. The independent memory
cap rejects a 512x1024 final angular grid even with inexpensive path orders.

Reproduce diagnostic timings with:
`cargo test -p nsb --release --lib computational_budget_reference_performance --locked -- --nocapture`.
Timings are medians of 21 accepted evaluations on an Intel Core Ultra 9 185H,
Linux x86_64; they are observations, never scientific acceptance thresholds.
All initial orders below are mu/azimuth/path, before refinement.

| Case | Layers | Initial orders | Refinements | Work units | Worst-plan buffer bytes | Median ms | Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Homogeneous | 1 | 16/32/16 | 4 | 80,045,454 | 3,149,824 | 0.799 | Accepted |
| Two-layer | 2 | 16/32/16 | 4 | 147,153,462 | 3,149,824 | 1.256 | Accepted |
| Forward HG g=0.85 | 1 | 16/32/16 | 4 | 80,045,454 | 3,149,824 | 5.572 | Accepted |
| Layered reference | 16 | 16/32/16 | 2 | 17,556,240 | 197,632 | 7.635 | Accepted |
| Excessive plan | 1 | 256/512/256 | 1 | 546,573,317 | 12,591,104 | 0.0206 (preflight only) | Rejected before sampling |
| Memory-only excess | 1 | 256/512/2 | 1 | 26,215,709 | 12,591,104 | — | Rejected before sampling |

## Numerical validation

Positive Gauss–Legendre nodes integrate mu over (0,1), periodic azimuth integrates
0–2π, and Gauss–Legendre path quadrature splits explicitly at every layer edge.
Each refinement evaluates angular-only, path-only and joint doublings; it checks
all against the coarse and joint results. Orders and refinement count are bounded.
Failure returns an error without an accepted partial result. Comparisons use
`abs(change) <= 1e-12 + 1e-4 * max(abs(values))` by default, with only tighter
caller tolerances accepted. These are fixture numerical targets, not rigorous
error bounds, and finite quadrature cannot certify arbitrary unresolved source
structure. Sources must be deterministic and return errors for missing samples.

An independent homogeneous oracle integrates the exponential path analytically,
using a nonpositive `expm1` divided difference and the equal-mu limit. Tests cover
zero/thin/moderate columns, equal/nearly equal mu, absorption and anisotropy.

The committed `tests/data/single-scattering/scipy-1.17.1.json` fixture is generated
offline by `scripts/generate-single-scattering-reference.py` with SciPy **1.17.1**.
It analytically eliminates azimuth and uses exponential integrals for incoming
mu plus QUADPACK adaptive path integration. It does not reuse NSB quadrature or
the homogeneous divided difference. Tests require no Python/network at runtime.
Matched fixtures use 450 nm, mu_out=0.8, isotropic aerosol g=0, Rayleigh and a
vertical synthetic dipole. Coefficients, layers, units and tolerances are pinned
in the fixture/generator.

| Fixture | Independent single | Rust single | Accepted orders (mu, azimuth, path/layer) |
| --- | --- | --- | --- |
| Homogeneous | 0.0007492254244742733 | 0.0007492254246676188 | 32, 64, 32 |
| Two-layer | 0.0007142897454565831 | 0.0007142897380897553 | 32, 64, 32 |

Relative differences are 2.58e-10 and 1.03e-8. Absolute angular-only/path-only/joint
changes are 2.64e-10/3.27e-16/1.04e-10 and
4.56e-10/1.08e-19/4.57e-10, respectively. The oblique, azimuth-dependent HG g=0.85
fixture refines to 64/128/64, with angular/joint change 4.84e-9 and path change
6.51e-19. An independently coarse initial path order of two refines to
128/256/16 for a depth-two column, with angular/path/joint changes
1.22e-13/1.42e-8/1.42e-8. A collimated quadrature contribution separately tests
the numerical path against equal/nearly equal-mu analytic limits. These results characterize the stated synthetic fixtures only.

Other tests cover vacuum, pure absorption, SSA endpoints/linearity, phase
normalization/HG first moment, forward/back sign, rotation symmetry, layer
split/merge, constituent separation, local-pressure/direct-column parity,
nonfinite/missing inputs, overflow, strict tables, explicit convergence failure
and repeatability. A downward flux budget bounds the admitted first-order flux
by extinguished incident flux; it does not assert exact first-order conservation.

## Limitations and release

No multiple scattering, polarization, spherical geometry, refraction, surface
reflection, internal emission, Gaia/HEALPix adapter, within-bin spectral invention,
LUT/cache acceleration or empirical accuracy claim. Frozen assets and reserved
holdouts are untouched. #187/#263 remain open; #192 owns later acceleration.
PR #264 is intentionally not merged and is not an implementation dependency.

Siderust is temporarily patched workspace-wide to immutable revision
`83d1ebb29ca1ae0b327d5784d13eda231d52c120`. See
[siderust-compatibility.md](siderust-compatibility.md) and
[#265](https://github.com/VPRamon/NSB/issues/265) for the registry publication
blocker, archive verification contract and exact transition to a released version.
