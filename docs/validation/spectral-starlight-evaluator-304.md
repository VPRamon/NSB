# Opt-in native spectral Starlight evaluation (#304)

`NsbEvaluator::evaluate_spectral_starlight(geometry, view, band, mode)` composes
an explicitly configured, checksum-verified spectral product with the existing
transport operators. It is experimental. Ordinary `evaluate()`, broadband totals,
planning, automatic selection, site calibration, Python and CLI outputs retain
their existing behavior. This capability makes no observational accuracy claim.

## Audit and integration decision

The integration was developed against main `7d3e5da`. `StarlightMap::integrate_band`
already integrates complete stored bins exactly and reports available source
statistical uncertainty separately. `StarlightProduct::with_experimental_map` and
`with_shared_experimental_map` already select loaded data. `NsbEvaluator` already
retains that map in an `Arc`, but its ordinary Starlight contribution is the
integrated component; restricted-band evaluation deliberately rejects it.

`StarlightSky` already validates the checksum-bearing spectral source, exact
native bin, geometry and 360–570 nm domain, and implements the verified local →
true equatorial → ICRS → Galactic route. Its input spectrum is explicitly
`WithinBinSpectrum::FlatPhotonDensity`. `starlight_healpix_single_scattering`
already returns TOA/direct/single with provenance and independent angular and
spectral checks. `projected_starlight_multiple_scattering` already returns the
full decomposition, physical-order convergence, boundary/kernel/diffuse/height/
wavelength diagnostics and bounded work/workspace. These are reused directly.
Historical resource blockers in #269 are superseded by #270's accepted projected
seven-bin experiment; they are not grounds for rejecting that configuration.

The smallest addition is a dedicated evaluator method with an explicit mode and
mode-specific result enum. Existing scattering result types remain intact.
Identity/direct need small result records because neither existing scattering
result can represent an uncomputed first order. Their source is a shared `Arc`,
including its original grid, full spectral coverage, maturity and provenance.
There is no second source adapter, map, scattering solver, cache or dependency.
The existing direct illumination operator is shared with a small bounded
complete-bin wavelength integral, so direct queries perform no angular solve.
Its numerical limits match the existing wavelength contract (at most 64 nodes,
250 million planned work units). No second attenuation follows any solver.

Product/epoch/bin validation remains in `StarlightSky`; atmospheric/phase coverage,
view domain, numerical controls, convergence and computational budgets remain
in the operators. The evaluator checks product availability and dispatches only
the explicitly requested capability. It never derives TT or UT1 from UTC.

## Runnable Rust workflow

```sh
cargo run --release --locked --offline -p nsb --example spectral_starlight_evaluator
```

The [complete example](../../crates/nsb/examples/spectral_starlight_evaluator.rs)
loads the existing AGPL synthetic fixture with its independently recorded SHA-256,
selects it using `NsbModelConfig::with_starlight_product`, constructs a geometric
Horizontal view and explicit TT/UT1 observation, declares an upstream
`LayeredAtmosphere` and phase, and executes identity/direct/single/multiple modes.
It prints separated radiances and numerical work, and handles a partial-bin error.
CI executes this example offline after the normal dependency build.

The central call is:

```rust,ignore
let result = evaluator.evaluate_spectral_starlight(
    geometry, view, [Nanometers::new(360.), Nanometers::new(390.)],
    SpectralStarlightMode::Single {
        atmosphere: &atmosphere, phase: &phase,
        assumption: WithinBinSpectrum::FlatPhotonDensity,
        angular, wavelength,
    },
)?;
assert!(result.higher_orders().is_none());
assert!(result.ground_total().is_none());
```

For external products substitute `StarlightMap::from_json_file(path, expected_sha256)`.
The expected hash must come from independently trusted product metadata. Loading
and evaluation have no network side effects. There is no implicit download.

| Mode / result variant | TOA | Direct | Single | Higher | Total scattered / ground total |
| --- | --- | --- | --- | --- | --- |
| Identity / `Identity(StarlightIncident)` | exact bin | unavailable | unavailable | unavailable | unavailable |
| Direct / `Direct(Box<StarlightDirect>)` | exact bin | computed | unavailable | unavailable | unavailable |
| Single / existing `StarlightTransport` | exact bin | computed | computed | unavailable | unavailable |
| Multiple / existing `ProjectedStarlightMultipleScattering` | exact bin | computed | computed | computed | computed |

Accessors borrow the same stored values, rather than duplicating the decomposition.
Only the accepted multiple result supplies `ground_total()`. Its value means the
accepted physical-order sum within the solver's declared numerical criteria.
A truncated single result cannot masquerade as full ground radiance. Physical
zeros can occur in computed scattering channels in vacuum or pure absorption;
`None` always means uncomputed. Solver failure returns an error without an
accepted partial result. Every radiance is typed photons/(cm² ns sr).

## Physical and spectral domain

Exactly one complete original native bin in 360–570 nm is accepted, including
synthetic variable-width bins. The XP candidate's seven 30 nm vacuum bins retain
asset hash `fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`,
verified against the current [final #270 evidence](gaia-xp-factorized.md).
This represents the admitted XP population, not all stellar light. Missing
population flux is unknown. The bundled 300–650 nm integrated product cannot be
used as a spectral surrogate. Partial bins, merged-bin requests, missing native
bins and extrapolation fail. Exact bin integration is distinct from the flat
photon-density approximation used during wavelength-dependent transport and
from its Gaussian quadrature change diagnostics. No solar colour is inferred.

Atmospheric modes are scalar, plane-parallel, geometric, black-ground transport
with zenith 0–80 degrees. The column must cover the whole bin, and scattering
phases must cover its endpoints as well as quadrature nodes. All incoming and
outgoing attenuation uses the same upstream constituent optical column. Rayleigh
and aerosol scattering remain distinct from absorption, with existing normalized
phases. DGL, internal emission, reflection, spherical geometry, refraction,
polarization and instrument acceptance are outside this API's declared model.
There are no options purporting to enable those physics. Their effects cannot be
interpreted as validated predictions from this scalar calculation.

`StarlightGeometry` requires a WGS84 geodetic observer and explicit TT/UT1 Julian
dates for the same physical instant. Callers supply supported time/Earth-orientation
information; NSB neither sets UTC=UT1 nor estimates missing offsets. Horizontal
altitude is geometric; azimuth increases clockwise from north. The atmosphere's
first edge defines the column origin in its caller-declared height datum. No
ellipsoidal-height/MSL equality or geoid correction is imposed. The example and
tests deliberately use unequal column and ellipsoidal heights.

HEALPix means are containing-parent-pixel radiances, not point-star, PSF, telescope
or aperture fluxes. No source supplementation, mask, smoothing or double counting
is introduced. Existing absolute ICRS/Galactic anchor tests remain in the full suite.

## Independent verification

`crates/nsb/tests/spectral_starlight_evaluator.rs` exercises real external public
interfaces, including checksum loading/selection, all stored fixture bins,
source-Arc identity, component availability, actual atmosphere retention, units,
convergence/provenance, zeros and unsupported requests. Analytical transmission
uses `I_bin exp(-tau/mu)` independently from production evaluation. Vacuum and
absorption agree within 1e-14 relative. Mixed constituent extinction is checked
against its independently summed analytical depth, with no double attenuation.

For the uniform first bin (TOA .003), Rayleigh depth .08, absorption .08,
view cosine .8, the independent 256-stream continuous-height PythonicDISORT
reference is the already pinned #269 `rayleigh_uniform` experiment, scaled by
linearity from its spectral TOA .025 to the fixture's .0001/nm. No new reference
study or production fit is introduced:

| Channel | New public evaluator | Independent reference | Acceptance |
| --- | ---: | ---: | --- |
| Direct | .002456192259233946 | .003 exp(-.16/.8) | 1e-14 relative |
| Single, layer-exact solver | .00010647771984888993 | .12 × .0008873157461357191 | 1e-4 relative |
| Single, finite-cell multiple solver | .00010635422459869226 | same independent single | 1% relative |
| Higher, multiple solver | .000012524066098767417 | .12 × .00010378276034380166 | 1% relative |

The 1% continuum allowance is inherited from independent solver validation, not
physical uncertainty. Tests additionally compare isolated bright and
horizon-crossing NSIDE1 pixels, with HG g=.65 and mixed optics, against the
existing independent SciPy/healpy angular references (1e-3 relative allowance).
The mandatory uniform fixture and both references are already redistributable
repository assets; no scientific asset bytes are modified.

Errors are exercised for checksum mismatch, integrated product, invalid/missing
bins, invalid epoch/site/view, atmospheric coverage, invalid controls, independent
wavelength/grid/order non-convergence and excessive work/workspace. The core
unit test separately exercises an evaluator with no configured Starlight source.

## Optional frozen seven-bin replay

The existing scientific-validation executable now accepts `--evaluator` alongside
`--projected`, using the new public method at identical inputs. It retains the
same detailed machine-readable records and fails on any rejected bin:

```sh
cargo run --release --locked -p nsb --example starlight_multiple_scattering -- \
  --evaluator --projected --cells=4 --orders=32 --kernel-mu=128 --kernel-azimuth=64 \
  /path/to/starlight_360_570_candidate.json 2459679.9675055384 2459679.966703672
```

The new flag only changes dispatch; it shares the loaded `Arc` and original
settings, column, phase, source, view and epochs with the previous public solver
path. `--evaluator` without `--projected` is rejected. Normal CI uses the small
fixture. The large original asset was not present in this development workspace;
no new full-map measurement is claimed. The current authoritative frozen sum is
single .002418873712783281, higher .0006678607407389825, scattered
.003086734453522263. The matched independent total is .003086121304126204.
Those numbers and the original ~42 s seven-bin runtime are inherited #270 evidence
under its exact frozen conditions, not new evaluator measurements or correction
factors. The new small-fixture path is independently tested through the evaluator.

## Performance and bounded execution

Measured 2026-10-26, Linux x86_64, Intel Core Ultra 9 185H, rustc 1.97.1,
locked release build, serial numerical kernels on a shared workstation. Compilation
excluded. The complete 360–390 nm bin uses the same fixture/model as above.
One process run of the example gave:

| Operation | Wall time | Conservative entire planned work |
| --- | ---: | ---: |
| Asset parsing and checksum verification | .129 ms | outside numerical solve |
| Evaluator construction | .397 ms | outside numerical solve |
| Identity | .601 ms | no angular/spectral quadrature |
| Direct | .301 ms | 652 |
| Single | .177 s | 7,071,655 |
| Multiple | 1.548 s | 10,451,220,736 |

These are individual post-load evaluations, including their astrometry and
acceptance checks, not a controlled throughput benchmark. The immutable map is
reused; there is no cross-query numerical cache. The first identity evaluation
includes any upstream lazy initialization. Maximum whole-process RSS from GNU
`time -v` was 20,172 KiB, covering loading/evaluator/all modes. The multiple solver's
conservative numerical workspace bound was 16,520,032 bytes, below 16 MiB;
whole-process RSS also includes immutable assets, allocator and result snapshots.
Direct quadrature holds at most 64 node/weight pairs (1 KiB payload). Single
retains the existing 8 MiB buffer guard, and multiple its 16 MiB/32 billion work
limits. The full plan is charged before source projection; tolerances are never
lowered silently. The physics/reference solver is unsuitable for an assumption
of interactive production point-query throughput. General acceleration is #192.

## Uncertainty, maturity and interface audit

The existing source `integrate_band` statistical channel is separate from
calibration systematics and population completeness. Identity/direct retain the
shared source for its existing uncertainty queries. Existing scattering results
retain their `uncertainty: None`: this integration does not propagate statistical
errors, unavailable cross-bin covariance, atmospheric parameter uncertainty,
missing population flux, calibration or model discrepancy into a fabricated
combined sigma. Numerical changes, discretization differences and order tails
are diagnostics, not confidence intervals. Experimental source/transport maturity
and site calibration are unchanged. No empirical admission is claimed.

The Python API currently wraps checksum-loaded Starlight maps and broadband
queries, but not `LayeredAtmosphere`, optical constituents, phase tables, explicit
TT/UT1 geometry or the scattering settings/results. Full parity would require
several new scientific binding contracts, rather than a thin method forwarding
existing bound values. This PR deliberately exposes the coherent Rust operation
and documents this Python limitation; there is no Python-only physics or CLI
change. Existing Python/CLI behavior is checked by their established tests.

The API change is additive. Existing signatures, default configurations,
300–650 nm Starlight, Zodiacal, Airglow/Oxygen/F10.7, Moonlight, planning and
site-calibration paths are unchanged. The public snapshot and historical SemVer
gate remain enforced. Broader architecture is #187; numerical acceleration is
#192; population/systematic/empirical admission is #263; instrument validation is
#306. No new follow-up issue is required for this bounded integration.
