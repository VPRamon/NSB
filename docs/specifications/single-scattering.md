# Experimental single-scattering reference (#190)

`nsb::transport::single::single_scattering` is an opt-in scalar, elastic,
plane-parallel reference. `cargo run -p nsb --example single_scattering` evaluates
an anisotropic wavelength-resolved synthetic sky. Existing evaluator and component
calculations do not call it. No default migration or empirical admission is implied.

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
