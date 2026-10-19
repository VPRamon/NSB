# Spectral Starlight transport validation (#267, PR #268)

Original reviewed PR SHA: `228013b9605716a08701b00e1f9d77824fc45f49`.
Main inspected: `dd59f5a7fb13d8edb98be33590a8cbe25b8bd61f`.
Siderust stays pinned to `83d1ebb29ca1ae0b327d5784d13eda231d52c120`;
#265 remains the registry release blocker. #264 is not a dependency.

## Datum and absolute-frame corrections

AFFN `Geodetic<ECEF>` height is WGS84 ellipsoidal; Siderust's profile column
altitude is MSL. Equality of those numbers was an invalid physical consistency
check. It is removed, with neither input altered. The first atmospheric edge
still defines the plane-parallel observer origin. No geoid model or altitude
correction is inferred. Regressions vary origins −430/0/2060/2097 m, geodetic
heights −20/0/2097/2132/4000 m, equal heights, finite/invalid geometry, columns,
coordinate bases and local pressure independently.

Historical comparison also exposed a reversed ICRS/Galactic transform in the
pinned dynamic Siderust frame catalog. The adapter now uses the existing
canonical static `TransformFrame` implementation, matching production Starlight.
No dependency/upstream change was needed. An absolute IAU Galactic-pole anchor
catches the defect that paired round-trip checks alone missed. TOD/ICRS and
Horizontal transforms remain upstream. Existing evaluator predictions are unchanged.

## Numerical root cause and correction

The old bounded smooth GL/periodic-azimuth path sampled discontinuous parent
pixels without conserving each parent's area. Its frozen-map diagnostic failed
all seven bins at orders 256/512/32 and ~212M configured work units/bin.
These historical failures are not accepted values; their geometry also predates
the absolute-frame correction. The smooth path remains supported for smooth
illumination, with its original independent path/angular tests.

The new public `starlight_healpix_single_scattering` streams RING child centers
while preserving native parent means. Equal child area, antipodal hemisphere
selection and subdivision of horizon parents remove the native-pixel aliasing.
Only the attenuation/phase kernel varies inside a parent. The shared #190 kernel
provides exact homogeneous-layer antiderivatives of its existing path integrand,
using Siderust coefficients/endpoint columns and Optica phases. This is a narrow
alternative angular quadrature, not another scattering model. See the
[specification](../specifications/starlight-transport.md) for formulas and budget.

Two successive angular changes plus raw whole-sphere phase normalization must
pass. There is no phase renormalization, horizon cutoff, relaxed tolerance,
source smoothing, correction multiplier or changed production optical law.
These are resolution diagnostics rather than certified error bounds. An
unresolved delta-like HG lobe fails explicitly instead of falsely accepting zero.

## Independent synthetic validation

Offline fixtures cover one/three/seven/fourteen bins and variable widths, exact
flat-density conversion, full-bin integration, independent wavelength failure,
vacuum, pure absorption, SSA endpoints, finite/invalid geometry, legacy/schema/
checksum/origin/source errors, NESTED rejection, source provenance and unknown
uncertainty. Existing #190 analytical, SciPy, angular/path and CLI/evaluator
regressions remain active. Exact-layer evaluation additionally matches the
unchanged SciPy homogeneous and heterogeneous two-layer fixtures, analytical
homogeneous columns, split-layer invariance and smooth GL integration.

The independent angular generator is
`scripts/generate-starlight-angular-reference.py`, pinned to healpy 1.20.1,
SciPy 1.18.1 and NumPy 2.5.4. It integrates Galactic z=sin(b)/longitude using
QUADPACK, independently located native-pixel boundaries and an explicit horizon
root, with SciPy `exprel` for the homogeneous path. It does not reuse Rust's
HEALPix implementation or child-center sum. The committed JSON means ordinary
CI needs no Python scientific packages or external map. Reproduction:

```sh
uv run --no-project --python 3.12 --with healpy==1.20.1 \
  --with scipy==1.18.1 --with numpy==2.5.4 \
  python scripts/generate-starlight-angular-reference.py
```

Cases cover isolated pixels above/below/crossing the horizon, bright forward
HG g=0.85, backward g=−0.65, isotropic aerosol and an anisotropic 12-pixel field.
Accepted results must match the independent integral within relative 1e-4;
zero below-horizon contribution is exact. Boundary scans 64→128 change results
by at most 1.4e-17; QUADPACK's largest reported absolute estimate is 5.9e-10
(not a rigorous bound). Separate independent visible-area integrals test
horizon parents. Parent counts at NSIDE1/2/3/4 and factors1/2/4 are exactly
factor²; hemisphere measure is 2π. Exact quarter-turn Galactic seam symmetry,
compatible coarse/fine constant-parent maps and deterministic repeatability
pass. Strict tables/double-HG, resource rejection and narrow-lobe failure pass.

## Frozen-pilot boundary and reproducibility

Pinned independent evidence:
[nsb-validation 71bfce5 report](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/reports/starlight-empirical-validation-final/report.md),
[scattering producer](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/scripts/scatter_xp_pilot.py),
[protocol](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/configs/xp-scattering-pilot-v1.json),
[boundary projection](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/src/nsb_validation/starlight/disort_boundary.py).

The original local 83,561,301-byte map was read without modification and its
SHA-256 verified as
`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
It is Galactic RING NSIDE128, seven integrated 30 nm bins from 360–570 nm.

Recovered reference conditions: DESI exposure 129304/b0; UTC start
2022-04-10 11:02:17.169, duration 1172.251 s, mid-exposure geometry; longitude
−111.59989°, latitude +31.96403°, height 2097 m; geometric fiber-zero altitude
76.9256911°, north-clockwise azimuth 271.8975544°, refraction disabled.
Pressure 779.5 hPa is a declared standard-atmosphere scenario, not a measured
exposure pressure. Aerosol tau550=0.03, Angstrom exponent=1.38, SSA=1,
HG g=0.65, zero molecular absorption; constant optical mixture with height,
black ground. Normalized Rayleigh phase has beta2=0.1; HG moments are g^l.
The historical Rayleigh column for wavelength `u` in micrometers is
`(779.5/1013.25)*0.008569*u^-4*(1+0.0113*u^-2+0.00013*u^-4)`.
This differs from Siderust's Bodhaine prescription; production coefficients
were not fitted or changed to match it.

The original nsb2 pin is `bc9320db03fbff69997ce366c6e76a37339c5d00`.
It sums above-horizon HEALPix illumination with bin-center kernels. The
independent scalar PythonicDISORT 1.8 reference instead conservatively projects
pixel contributions into GL cells and cosine angular moments centered on the
view, then extracts a first-order limit. Its full scattered reference includes
higher orders. The case-specific integrated values, photons/(cm² ns sr), are:

| Historical computation | Value |
| --- | ---: |
| nsb2 first order | 0.0025991471011773495 |
| Independent first order | 0.0025990756849478282 |
| Independent multiple scattering | 0.0033296021852152395 |

The example declares atmospheric origin 2097 m independently of ellipsoidal
height 2097 m; numeric coincidence does not verify geoid consistency.
TT JD2459679.9675055384 and UT1 JD2459679.966703672 were computed with
Astropy 8.0.2 and bundled offline IERS data. The historical source directions
use Astropy Galactic→AltAz (including apparent-place effects), whereas this
adapter uses a geometric Siderust celestial rotation. Exact IERS/version,
aberration and frame-convention identity are not claimed. An offline Astropy
8.0.2 comparison at all 196,608 NSIDE128 centers gives geometric/apparent
direction residuals 0.0847 / 17.8393 / 21.5749 arcsec (minimum/median/maximum).
The geometric rotation has 98,304 above-horizon centers versus 98,307 for
Astropy. Those differences are disclosed, not absorbed into optical coefficients.

## Frozen Gaia results after both corrections

All seven current-model bins accept angular NSIDE **512** (128→256→512),
wavelength order **4** (2→4), relative targets 1e-4, and absolute targets 1e-12
in the corresponding spectral/integrated units. No partial total is accepted.
The path is layer-exact, so it has no path-order error. Largest accepted spectral
angular changes are 1.96e-12 latest and 1.77e-11 previous; raw whole-sphere
phase integrals are Rayleigh 0.99999996777, aerosol 1.00000006420. A
separate monochromatic diagnostic (not a substitute full-bin plan) at each
bin center continues through NSIDE1024: maximum relative changes across bins
are 1.53e-6 (128→256), 2.27e-7 (256→512), 7.60e-8 (512→1024).

| Bin nm | Current full-bin single, photons/(cm² ns sr) |
| --- | ---: |
| 360–390 | 0.00024993575191143786 |
| 390–420 | 0.0003445674547143664 |
| 420–450 | 0.00036575299729958667 |
| 450–480 | 0.000402236185339468 |
| 480–510 | 0.0003642020711950665 |
| 510–540 | 0.00034446205816497145 |
| 540–570 | 0.00034845046748063226 |

| Deliberately distinct experiment | Integrated single |
| --- | ---: |
| Current Siderust Bodhaine, full-bin quadrature | **0.00241960698610553** |
| Same current optics frozen at bin centers (`--current-bin-centers`) | 0.00241726783764973 |
| Historical optical prescription frozen at centers (`--pilot-bin-centers`) | **0.00259905749633912** |

Full-bin integration changes the current center approximation by +0.0967683%.
At fixed centers/geometry/integrator, the current versus historical optical
prescriptions differ by −6.99445%. The historical-law compatibility fixture is
−34.4747 ppm versus nsb2 first order, −6.99811 ppm versus the independent first
order. Those residuals mix remaining astrometry and angular-projection effects;
they are not a proof of exact historical identity or empirical accuracy. The
historical multiple-scattering value is never used as a correction factor.
Production coefficients and evaluator defaults were not fitted or changed.

Release measurements on Linux x86_64 / Intel Core Ultra 9 185H, shared
workstation: current-model transport takes **2.53–3.43 s/bin** (map loading and
compilation excluded). Each complete configured one-layer bin plan costs
**243,600,079** conservative work units, below the unchanged 250M cap. It
precharges all configured wavelength nodes, even those unused on early
acceptance. No whole-sky subpixel buffers are allocated; layer responses,
phase-audit descriptors and wavelength quadrature stay under 8 MiB.

Reproduce with the commands in the specification; append the explicit mode
flag only for the corresponding center fixture. The frozen map was never
regenerated. Its checksum is rechecked after the runs.

## Quality gates and limitations

Final local and GitHub verification is recorded in PR #268. Historical 19/19
CI and 199/202 coverage at the original SHA are superseded and are not evidence
for this follow-up. Coverage uses the existing LLVM diff/overall policy with
unchanged exclusions and thresholds, against main and the original reviewed SHA.

The conservative HEALPix prerequisite for #192 is implemented and independently
validated for these supported cases; broader LUT/cache/acceleration work remains
open. Exact historical apparent-place replay is a distinct follow-up, not a
blocker to these explicitly geometric first-order results. Higher orders,
source systematics, unknown cross-bin covariance, empirical admission and
spherical/polarized/reflected transport remain outside this PR. #267 stays open
until human-approved merge; #265 still blocks registry publication.
