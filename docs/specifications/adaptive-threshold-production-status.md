# Adaptive threshold discovery: scientific contract and migration

PR [#235](https://github.com/VPRamon/NSB/pull/235), issue
[#234](https://github.com/VPRamon/NSB/issues/234). This revision implements
usable adaptive searches with every physical component and both lunar models.
The former certificate-only experiments are historical evidence, not performance
measurements of this algorithm. The PR remains Draft during acceptance testing.

## Three distinct supported search contracts

| Interface | Discovery contract | Failure behavior |
| --- | --- | --- |
| `periods_below_threshold[_with_context]` | Frozen discrete scanner, with the caller's `sample_step` and original physical windows/grid phase. | Existing model errors propagate. Sub-step excursions can be missed. |
| `periods_below_threshold_adaptive[_with_context]` | Step-free, physics-partitioned adaptive discovery under the resolved-model assumptions below. Authoritative radiance probes and crossing refinement. | Numerical ambiguity or resource exhaustion returns `Unsupported`, without a partial schedule. |
| `periods_below_threshold_certified[_with_context]` | Closed-interval classification by sound envelopes; no approximate acceptance or scan fallback. | Any unresolved eligible time returns `Unsupported`. |

The new adaptive methods are normal compiled library functionality, also exposed
in Python and CLI `window --adaptive`. They ignore `sample_step`. The legacy
methods remain sampled during the frozen-API transition. Neither a polynomial
prediction nor a validated adaptive schedule is an interval certificate.

A successful adaptive result is a usable schedule under its model-resolution
contract. It is **not mathematical proof of continuous-time completeness**.
Finite evaluations cannot exclude an arbitrarily narrow, unobserved feature of
an arbitrary black-box function. Same-side probes, fit residuals, and polynomial
root counts are never promoted to certificates in the strict engine.

## Implemented adaptive algorithm

1. Reuse `SiteWindowContext`, cached target Starlight, Siderust Sun/Moon/target
   altitude events, and the established integrated radiance evaluator.
2. Partition physical eligibility windows at astronomical-night edges, PALACE
   local-solar hour/month boundaries, UTC date boundaries, F10.7 forecast
   issuance and retrieval instants, Moon visibility edges, and target horizon
   branches even when the caller clears visibility filters. PALACE's retained
   night-third identity does not change its radiance formula, so the adaptive
   path avoids those redundant fits; the legacy grid remains unchanged.
3. Partition Zodiacal solar-longitude table cells and corner-fill edges at
   5-degree offsets, folding points, and the 30-degree reddening transition.
   Use Siderust solar longitude with a bracketed scalar solve, not a replacement
   solar ephemeris. Daily longitude advance must be finite and <=2 degrees;
   otherwise this temporal regime is unsupported. Reddening's 90-degree edge
   is already in the 5-degree set. Fixed ecliptic latitude needs no time search.
4. First try the admitted cached-scalar/Moon-down certificates. Otherwise fit a
   degree-four Chebyshev interpolant on five Lobatto nodes, checked at **all four**
   interlaced nodes and independent one-third/two-third interior nodes. Upgrade
   to degree eight with four additional independent validation nodes when
   necessary, then subdivide on disagreement. High-fit-only checks are lazy
   to avoid spending those physical evaluations on resolved low fits. Intervals longer than
   six hours subdivide before fitting to resolve diurnal geometry. This is an
   initial model-resolution safeguard, not a fixed radiance scanning step.
5. Probe polynomial roots **and derivative roots** with the authoritative NSB
   model. Near-threshold extrema get an exact local golden-section search to
   0.1 ms, conditional on a resolved unimodal local basin. Heights within
   128 machine epsilons of threshold after normalization are numerically
   ambiguous and return an error. A truly constant certified contribution
   equal to threshold remains accepted.
6. Retain all fit, validation and extremum samples. Refine every discovered
   sign transition with safeguarded Illinois/secant proposals and forced
   bisection, using exact signs. Stop only with a bracket <=0.1 s. Use its known
   accepted endpoint, excluding the bracket interior; never export its midpoint
   as known accepted. At genuine jumps this is a transition bracket, not a
   continuous root or uniqueness claim.
7. Coalesce accepted intervals deterministically. There is no production
   discrete fallback in the adaptive path. Unsettled fits, precision limits,
   model errors and exhausted budgets fail without returning a partial result.

Fits operate on interiors inset by at most 1 ms. Actual physical endpoints are
probed separately, so a polynomial never has to interpolate both sides of a
known jump. An isolated accepted endpoint does not become a positive-duration
observing period. This matters at twilight: the legacy midpoint refiner can
manufacture <=0.432 s accepted intervals whose interior radiances exceed the
threshold. Comparisons explicitly validate and classify these artifacts.

## Assumptions and limits

The practical contract assumes that the implemented piecewise physical signal's
threshold topology is resolved by the checked approximations, authoritative
root/extremum probes and subdivisions, and that each optimized extremum basin
is unimodal at the working scale. The solar/lunar geometry and diurnal altitude
variation motivate this strategy; the empirical matrix tests its consequences.
**Observed agreement is not a rigorous interpolation error bound.** No claim
is made that all possible model configurations, dates or thresholds satisfy
these assumptions. A hidden feature that aliases every probe can still be
missed without triggering an error. No universal non-regression claim against
all possible 600-second grids is made.

The comparison contract is instead reproducible and finite: preserve every
600-second reference crossing witness in the documented workloads within the
combined refinement allowance, independently compare schedules with finer
scans, investigate every material discrepancy, and use analytically controlled
signals for discovery ground truth. Finer scans are empirical references, not
proofs. Additional adaptive events require authoritative validation; known
singleton endpoint artifacts require exact boundary/interior checks rather
than a blanket duration filter. The boundary allowance is 0.533 s: half the
legacy 0.864 s bracket plus the adaptive 0.1 s accepted-side bracket and floating
point conversion allowance. It does not excuse missing whole events.

## Physical component status

| Component | Adaptive behavior | Rigorous interval certificates |
| --- | --- | --- |
| Starlight | Reuse the exact prepared scalar; compose in the same evaluator order. | Cached scalar, including equality. |
| Zodiacal | Full Leinert table/corner rules, reddening, Noll or no extinction, horizon behavior and sampled spectral integration. Cell/jump partitioning plus checked adaptive fits. | Active temporal envelopes unavailable. |
| Airglow | Full PALACE local hour/month climatology, monthly F10.7 resolver, time-valid data transitions, geometry, atmosphere/scattering and input validation, including outside-night validation. | Active temporal envelopes unavailable. |
| K&S 1991 Moonlight | Full existing phase, separation, horizon/airmass and analytic scattering model; exact candidate validation. | Prepared Moon-down skips only. |
| Jones 2013 Moonlight | Full distance/phase, spectral grids, scattering, atmospheric propagation, positive-node retention and integration branches. Continuous table kinks trigger fit upgrades/subdivision; unresolved numerical branches fail. | Prepared Moon-down skips only. |
| Combined masks | All 16 bit combinations, including configured Starlight and both lunar models, use the same composed authoritative evaluator. | Only compositions fully covered by existing certificates succeed. |

No physical brightness formula, spectral integration rule, photometric unit,
atmospheric coefficient, scientific asset policy or ephemeris is replaced.
Noll Zodiacal transmission is not assumed <=1; Airglow scattering is not
assumed attenuating. These would be unsound bounds. Strict active-model
certification continues to require separate validated temporal envelopes.

The frozen `ComponentMask::ALL` / `DEFAULT` policy is unchanged: this build uses
bits 13 (Zodiacal/Airglow/Moon), with Starlight opt-in unless an admitted bundled
production product exists. Explicit bits 15 are tested with a configured
experimental map; a synthetic map is not relabeled an admitted production map.

## Resource and representation limits

Adaptive queries allow at most 25,000 initial physical intervals. Define physical
complexity `C = sum(max(1, ceil(interval_days / 0.25)))`. The **whole query** may
spend at most `min(2_000_000, 64*C + 65_536)` exact evaluations and
`min(250_000, 8*C + 4_096)` polynomial fits. Saturating arithmetic avoids integer
overflow for extreme extents. Initial classification is reserved by complexity;
annual queries are not restricted to a fixed small number of initial probes.

Windows run in deterministic batches of at most 64. Each batch divides the
remaining budget equally, capped locally at 4,096 exact evaluations / 256 fits
and depth 32. Unspent work is reclaimed for later batches. No racing atomic
budget chooses which windows succeed; costs and schedules are independent of
Rayon worker count. Any exhaustion aborts the entire query with an explicit
error. No partial schedule escapes. Cache size and stack depth are bounded per
worker; pending interval work is at most 64 plus worker stacks. Output memory
is bounded by the initial windows and operation limits. These are operation
limits, not hard wall-clock preemption of a physical evaluator call. Initial
astronomical preparation keeps its existing Siderust resource semantics.

Strict discovery separately counts all initial classifications and at most
2,048 refinement-bound calls. Initial windows are never charged against that
refinement allowance; annual queries with >2,048 physical windows are supported.
Exact/fit limits, unsupported bounds and numerical errors remain distinct.

Native Rust UTC endpoints must round-trip inside the accepted TT interval and
query UTC extent; bounded inward adjustments are <=0.1 s. Python applies its
own 1..9999 year/leap-label restrictions and checks microsecond truncation,
with a separate <=0.1 s inward representation allowance. CLI checks its actual
**millisecond** formatting precision and contracts endpoints inward before
writing any output. Collapsed or unrepresentable periods fail explicitly.
Rust core semantics are not constrained by Python's datetime year range.
These checks contain representation effects, not astronomical time-scale error.

## Migration and validation evidence

Rust and Python add only `periods_below_threshold_adaptive` and its context
variant. CLI adds `--adaptive`, conflicting with explicit `--step` and
`--certified`. The example uses the adaptive API. The frozen sampled API,
query builders and Python constructor remain compatible; explicit legacy
sampling fields still receive their constructor's existing validation.
Rust physical-query/context validation is independent of legacy step validation.

Do not remove/deprecate `sample_step`, switch the existing method's discovery
contract, or remove CTAO service parameters silently. A subsequent compatible
release/migration decision must make the new contract explicit to consumers.
`nsb-service-ctao` is unchanged by this PR.

The committed tests cover analytical close pairs, a narrow grazing Gaussian,
multiple extrema, flat/equality behavior, numerically ambiguous tangency,
resource failures, every mask and lunar model, actual ALL, context reuse and
Rayon determinism, legacy grid phase, F10.7 retrieval jumps, local month/hour
boundaries, Zodiacal corner/reddening cells, near/far Moon targets, unfiltered
horizons, leap boundaries and inward output precision. A real Zodiacal ~53 ms
excursion missed by a 10 s scan is independently confirmed by a local 1 ms scan.

Historical certificate-only measurements remain in
[review benchmarks](adaptive-threshold-review-benchmarks.md) and
[compatibility CLI measurements](adaptive-threshold-cli-benchmarks.md).
They must not be cited as performance evidence for active adaptive search.
Complete usable adaptive benchmark results and final acceptance status are
recorded in the companion production benchmark report after measurement.


## Aliasing investigation and two-threshold correction

The original degree-four path checked only seven radiance nodes, with its two
validation points close to the endpoints. A reproducible compact C-infinity
pulse centered at 7,200 seconds in a six-hour interval is **visible to scan600
but invisible to every original probe**. The original implementation returned
one false-valid period; the new permanent regression requires two accepted
periods separated by the exact physical excursion. Symmetric 14,400-second and
omitted-interlaced-node cases are retained as additional regressions. The
recorded pre-fix failure was `hidden pulse at 7200: left 1, right 2`.

The fix checks all interlaced nodes, adds independent interior validation, and
retains every evaluation for transition discovery. It introduces no global
600-second grid or scan fallback. Increasing validation diversity fixes these
specific aliases; it cannot rule out every possible compact pulse.

A global derivative or curvature certificate would need outward bounds through
astronomical transforms, spectral integration, extinction and each tabulated
branch. The current scalar APIs do not expose those enclosures. Starlight has
an exact scalar certificate; the time-varying components do not. Zodiacal cell,
reddening and horizon branches and Airglow calendar/data jumps are partitioned;
K&S scattering/airmass and Jones spectral interpolation remain the exact
models. Their local geometry variation motivates checked fits and extrema
isolation, but it is not a proven interpolation error bound. A finer uniform
scan would give discrete coverage only and is deliberately not hidden inside
the adaptive implementation. The physical corpus tests these resolution
assumptions separately from the synthetic pulse regressions.

`periods_in_band_adaptive(query, minimum)` and its context variant solve the
inclusive condition `minimum <= NSB(t) <= query.threshold` on a **shared physical
fit and cache**. Both threshold levels generate exact crossing brackets. Each
bracket contracts toward its own accepted side; intersections exclude both
uncertain interiors. Endpoints are checked against both levels before export.
A band narrower than the combined crossing brackets may yield no positive
period. Isolated equality never manufactures duration; a genuinely constant
admitted value equal to both thresholds is accepted. Numerically ambiguous
extremum equality fails explicitly as in the single-threshold API.

CLI `--min-nsb` uses joint search even without `--adaptive`; `--step` remains
accepted for compatibility but does not control band discovery. This is a
correctness repair to the existing minimum constraint. CLI `--certified` with
a minimum uses `periods_in_band_certified_with_context`: the entire band needs
a sound enclosure, otherwise it fails. It never subtracts independently
contracted schedules. Python exposes the same band methods and projects output
inward at datetime precision. CLI tests validate the displayed millisecond
boundaries with the authoritative physical evaluator.

## Siderust 0.14 migration and acceptance artifacts

The workspace uses registry Siderust 0.14.0, Tempoch / tempoch-core 0.10.0 and
Principia 0.6.0. Optional Rust bridges and Python runtime distributions use 0.4.0.
No incompatible duplicate public time/coordinate package is introduced, and no
Git override is required. The dependency-only scanner comparisons A/B are
identical across all 67 exported cases; see
[Siderust compatibility](siderust-compatibility.md). Adaptive-versus-scanner
comparisons use **the same 0.14 graph** and are reported independently.

The production harness exercises native UTC exports, both CTAO sites, both
lunar models, all masks, actual ALL, explicit four-component composition,
1/7/30/365 days and 1/10/100 targets sharing a context. It emits a row only after
600-second witnesses and fine-reference topology/boundaries have been checked.
The report records overhead as well as improvements; incomplete searches are
never benchmarked as faster usable schedules. The PR stays Draft while the
full matrix and current-head CI are being evaluated.


### Split-coordinate regression found by the full matrix

The CTAO-N 30-day Moon-only reference exposed a representation discrepancy in
both lunar models. A fitted active-Moon period and a certified Moon-down period
met at UTC midnight. Flattening the physical endpoint to one absolute MJD float
lost its low split-coordinate part, leaving an artificial sub-microsecond gap
and fragmenting one accepted eight-hour period into two. This was neither a
missed physical excursion nor a dependency-upgrade radiance change.

Adaptive coordinates now use days relative to each original physical start,
retain both exact physical endpoints and construct probes through upstream
quantity/time arithmetic. A permanent 30-day regression for both lunar models
requires the unfragmented schedule and the same fine-reference boundaries.
Adjacent representable MJD endpoints are now supported, while genuinely
collapsed local interpolation nodes still fail explicitly. Native band tests
also retain split coordinates when sampling exported boundaries; rounding a
boundary back to one absolute float can itself step outside its accepted
interval at a model jump. CLI/Python tests evaluate the actual exported values.
