# Adaptive threshold discovery: scientific contract and migration

PR [#235](https://github.com/VPRamon/NSB/pull/235), issue
[#234](https://github.com/VPRamon/NSB/issues/234). This revision implements
usable adaptive searches with every physical component and both lunar models.
The former certificate-only experiments are historical evidence, not performance
measurements of this algorithm. The complete 204-case finite acceptance matrix
now passes; final release readiness is gated by the current PR-head CI checks.

## Production search contracts

| Interface | Discovery contract | Failure behavior |
| --- | --- | --- |
| `periods_below_threshold[_with_context]` | Step-free adaptive discovery, identical to the explicit adaptive aliases below. `sample_step` is ignored. | Numerical ambiguity, model errors and resource exhaustion propagate without a partial schedule. |
| `periods_below_threshold_adaptive[_with_context]` | Step-free, physics-partitioned adaptive discovery under the resolved-model assumptions below. Authoritative radiance probes and crossing refinement. | Numerical ambiguity or resource exhaustion returns `Unsupported`, without a partial schedule. |
| `periods_below_threshold_certified[_with_context]` | Closed-interval classification by sound envelopes; no approximate acceptance or scan fallback. | Any unresolved eligible time returns `Unsupported`. |

Normal Rust/Python methods and CLI `window` now use adaptive discovery in 0.1.
Explicit adaptive methods and `--adaptive` are aliases. Public step inputs remain
accepted for source compatibility and are ignored. The legacy scanner exists
only under `cfg(test)` for independent scientific comparisons. Neither a polynomial
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
   path avoids those redundant fits; the test-only legacy reference grid remains unchanged.
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

Strict search uses the same partitioning, orchestration and bounded budgets.
It accepts only sound whole-interval enclosures; it never falls through to an
empirical fit. Unsupported or inconclusive envelopes return an explicit error.
The earlier certificate-subdivision implementation remains a test-only oracle,
not a second production engine.

Native Rust UTC endpoints must round-trip inside the accepted TT interval and
query UTC extent; bounded inward adjustments are <=0.1 s. Python applies its
own 1..9999 year/leap-label restrictions and checks microsecond truncation,
with a separate <=0.1 s inward representation allowance. CLI checks its actual
**millisecond** formatting precision and contracts endpoints inward before
writing any output. Collapsed or unrepresentable periods fail explicitly.
Rust core semantics are not constrained by Python's datetime year range.
These checks contain representation effects, not astronomical time-scale error.

## Migration and validation evidence

Rust and Python normal methods use adaptive discovery; explicit adaptive names
remain aliases. CLI `window` defaults to the same engine; `--adaptive` is an
optional alias, and `--step` is accepted but ignored. `--certified` selects the
stronger interval contract. The public field, builder and Python constructor
argument are retained. Python still validates its legacy constructor argument;
Rust searches ignore the field even when zero or non-finite.

The maintainer explicitly authorized this behavior change in the 0.1 line on
2026-10-13. Signature snapshots and historical SemVer checks remain active.
`nsb-service-ctao` is unchanged; its separate migration should deprecate sampling
parameters and align its Siderust/Tempoch dependencies.

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
Complete usable adaptive benchmark results and the default decision are in
[the 204-case report](adaptive-acceptance/production.md),
[performance analysis](adaptive-acceptance/performance-analysis.md) and
[validation record](adaptive-acceptance/validation.md).


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
never benchmarked as faster usable schedules. Production readiness requires the full matrix and current-head CI. The
maintainer has explicitly authorized the 0.1 behavior migration.


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

## Default and release compatibility decision

The maintainer authorized reopening the behavior freeze and adopting adaptive
search in the 0.1 line on 2026-10-13. Normal Rust/Python methods and CLI `window`
therefore use the primary adaptive engine. Public signatures, step fields and
builders remain; ignored step values cannot change the returned schedule.
The old scan/refinement code is compiled only for independent tests and
benchmarks. Strict certification uses the same production engine in a mode
that accepts only sound enclosures. No service implementation is changed.

This is a behavioral migration: callers can receive additional narrow events,
corrected discontinuity handling and inward boundary placement instead of
midpoint estimates. It is not made compatible merely by unchanged signatures.
API_FROZEN, snapshot checks and historical SemVer checks remain in place.

The required Siderust update changes public temporal type identity from Tempoch
0.9 to 0.10. Downstream Rust consumers must align direct Tempoch requirements
and recompile. Python uses the released Siderust/Tempoch 0.4 bindings. Signature
checks cannot prove cross-version public dependency type identity. The version
remains 0.1.0 under the explicit maintainer authorization.

Production readiness still requires the complete scientific/performance matrix
and required checks. Empirical adaptive success is never a completeness proof;
active-model strict interval envelopes remain unavailable.

### A real short Airglow/Moonlight excursion

The CTAO-N seven-day Airglow + Jones case starting 2026-01-03 exposed a
2.4006-second rejected excursion immediately before a PALACE hour transition.
The 30-second reference scanner joined the accepted time on both sides;
adaptive discovery excluded the excursion. Authoritative midpoint values are
above the 0.25 threshold, and an independent 1-millisecond local scan agrees
with both adaptive accepted-side boundaries within 0.101 seconds. The permanent
regression preserves this improvement alongside the 53-millisecond Zodiacal
case. It is not an upstream dependency effect.

The benchmark comparator permits a reference interval to correspond to multiple
adaptive pieces only when a separate dense local scan confirms **every** added
rejected gap and both boundaries. Local steps are at most 10 milliseconds and
boundary disagreement must be at most 0.106 seconds, which is tighter than the
unchanged 0.533-second global reference allowance. Raw count differences and
confirmed missed reference excursions are reported separately; unexplained
extra/missing periods still fail. This is empirical corroboration, not an
interval proof or permission to ignore a topology mismatch.


### Test execution profile

The normal workspace test profile uses optimization level 1 with debug assertions
enabled. Dense physical-model and representation regressions run all the same
samples and assertions; no scientific tolerance or coverage is reduced. This
keeps `cargo test --workspace --all-targets --locked` and CI practical without
turning physical correctness tests into ignored release-only checks. Production
benchmarks continue to use the release profile.

### Twilight reference false positive

CTAO-S, ALL, Jones, the 30-day batch's target 36 (RA 353.912604 degrees,
Dec -5 degrees) exposed a longer discrete-reference error. At TT MJD
61050.04434116678, PALACE's astronomical-night boundary has an accepted endpoint
(radiance about 0.09253) followed immediately by about 0.25045. The radiance
remains above the 0.25 threshold for approximately 87.6 seconds before crossing
back below. Scan120 samples the accepted endpoint and a point after the crossing,
so it falsely accepts that initial duration. Adaptive correctly contracts past
the rejected time. A separate 10-millisecond local scan resolves both transitions
within 0.106 seconds; the permanent regression checks the exact endpoint,
multiple rejected interior values and the final accepted boundary.

The comparator now also reconciles a shifted fine-reference edge only when it
is an original physical-window endpoint and the entire added rejected excursion
is independently resolved locally. This is distinct from the old <=0.432-second
singleton midpoint artifacts. The same reference-miss counter records these
resolved edge excursions; unexplained boundary shifts continue to fail.

Complete process timings, separate from the library matrix, are recorded in
[production CLI measurements](adaptive-acceptance/cli-production.md).


### Validation infrastructure cleanup

The superseded certificate-feasibility timing harness is removed after replacement
by the complete practical schedule matrix. Its cached-Starlight certificate test,
shared physical fixtures and all scientific certificate/discovery regressions are
retained. Historical reports retain their recorded commit references. Test-only
modules now reside in the established `planning/tests/` layout with explicit
`cfg(test)` module registration. They remain part of ordinary test execution;
no coverage floor, LCOV exclusion or production API policy is changed.

### Deterministic error selection

Rayon's `Result` collector retains the first error observed by its workers.
A delayed failure in the first physical window could therefore lose to a later
window's error. A permanent one/two/four-worker regression reproduced that
behavior. Batches now collect indexed per-window results and propagate errors
in physical-window order. Successful fitted schedules, model calls and budgets
are unchanged; a failed query may finish its already bounded batch, and still
returns no partial schedule.

### Annual reference misses of accepted islands

The annual 100-target comparisons also exposed accepted islands entirely missed
by scan120: South target 30 (both lunar models), North K&S target 70 and North
Jones target 20. Independent 1 ms local references resolve approximately
7.199, 9.843 and 13.788 seconds respectively. Exact radiances are below 0.25
inside and above outside; both adaptive boundaries agree within the unchanged
0.106-second local allowance. The permanent regression preserves the original
annual physical-window/grid phase while restricting evaluation to the relevant
regime. [Recorded values](adaptive-acceptance/accepted-islands.json) retain the
independent diagnoses.

The fine comparator separately counts confirmed accepted islands and rejected
gaps. An unmatched island is accepted as a reference miss only when it is
disjoint from reference periods and a separate <=10 ms local scan, preserving
eligibility constraints, confirms its entire topology and both boundaries.
Other extra intervals still abort acceptance. Validation precedes expensive
timed repetitions, and every timed schedule must reproduce its validated
warmup exactly. This adds empirical comparison evidence, not a new certificate
or a change to the production discovery algorithm.

### Complete production acceptance and decision

All 204 usable scenarios passed their coarse-witness and fine-reference checks,
with zero unexplained discrepancies, exhausted budgets or timed-output mismatches.
The matrix independently confirmed 105 rejected-excursion and 13 accepted-island
reference misses, plus 12,317 fine / 12,391 coarse singleton midpoint artifacts.
These are scenario occurrences, not deduplicated astronomical events.
[Individual discrepancy records](adaptive-acceptance/confirmed-discrepancies.jsonl)
retain target labels and local-reference requirements.

Annual 100-target adaptive medians are 450–551 seconds versus 185–221 seconds
for scan600, paired ratios 2.39–2.50 under the recorded limited core assignments.
The physical-call ratios are 2.17–2.28; counters identify validation/root/extremum
work rather than uncontrolled subdivision. Peak pending intervals are 64,
recursion depth 6, active sample payload 9,024 bytes per worker, and cumulative
whole-benchmark RSS about 24.1 MiB. Counts and durations aggregate target queries;
budgets apply separately to each `ThresholdQuery`. Full CLI processes have
annual single-target adaptive medians 10.93–14.78 seconds and paired ratios
1.60–1.87, including construction, startup and I/O.

This is an explicit accuracy/cost tradeoff, not a general speedup claim. Complete,
bounded batches finishing in 7.5–9.2 minutes are accepted for offline annual
planning, with independently established discovery improvements. Under the
maintainer's authorization, adaptive therefore becomes the normal 0.1 engine;
legacy scanner/certificate-subdivision code remains only as independent test
references. Strict certification keeps its stronger enclosure contract.
