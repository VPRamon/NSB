# Adaptive threshold discovery: #234 feasibility spike

Status: internal executable prototype; **#234 remains open**. All new numerical
code is compiled under `cfg(test)` inside the existing NSB crate. No production
algorithm, physical model, public signature, CLI option or Python contract is
changed. This is the first scientific feasibility gate, not a new normal API.

## Baseline and investigation

Base: `a3f675245b4407c229a425786c6e6561d640ec30`, latest main when the branch was
created. Reviewed [#234](https://github.com/VPRamon/NSB/issues/234), its
[technical comment](https://github.com/VPRamon/NSB/issues/234#issuecomment-6123672116),
[#160](https://github.com/VPRamon/NSB/issues/160),
[PR #161 and reviews](https://github.com/VPRamon/NSB/pull/161),
[PR #50's narrow-excursion finding](https://github.com/VPRamon/NSB/pull/50#discussion_r3522977978),
[#37](https://github.com/VPRamon/NSB/issues/37),
[#170](https://github.com/VPRamon/NSB/issues/170),
[#175](https://github.com/VPRamon/NSB/issues/175), and
[#204](https://github.com/VPRamon/NSB/issues/204), including their comments.

The public search samples each physically partitioned interval using the full
prepared integrated evaluator. Equal-sign samples cannot exclude a pair of
crossings between them. Its safeguarded secant/Illinois refinement targets
`1e-5 day` (0.864 s), with at most 24 iterations. That target is not a universal
postcondition: the solver returns the final bracket midpoint even if its
iteration cap is reached. Across a discontinuity, a sign-changing bracket is
not evidence for a continuous zero. Decreasing the scan step is a diagnostic
resolution choice, not a proof of completeness.

Siderust 0.13.0's `event/search/crossings.rs`, `event/altitude/search.rs`, and
`event/stellar/altitude_periods.rs` were inspected from the pinned registry
source. Siderust uses Chebyshev candidates, precise refinement and local scan
fallback; its fixed-star engine uses hour-angle predictions with precise
validation. Its internal tail/residual heuristics are not an enclosure for an
NSB sum. This spike reuses the same `cheby` 0.4.2 numerical dependency as a
**dev-dependency**, and continues delegating all astronomical events to
Siderust. No new dependency version is introduced into Cargo.lock.

## Implemented mechanism

`planning/adaptive.rs` accepts ordered, non-overlapping TT MJD intervals, an
authoritative radiance callback, an optional enclosure callback, and internal
resource/precision controls. It processes intervals deterministically:

1. Validate an enclosure `[lower, upper]` for the **closed** interval.
2. Certify below only when `upper <= threshold`; certify above only when
   `lower > threshold`.
3. For ambiguous intervals, optionally fit degree-eight Chebyshev data at nine
   interior Gauss nodes. Evaluate predicted roots and interval endpoints with
   the authoritative callback. Use exact opposite-side probes to produce
   bisection brackets. Retain both authoritative endpoint values and an explicit
   `tolerance_met` flag.
4. Use an interior polynomial candidate to guide subdivision when it provides
   reasonable progress; otherwise bisect. All child intervals still require
   certificates. An empty/failed fit never accepts an interval.
5. Retain unresolved regions at width, depth, floating-point, or interval-budget
   limits. Exact-evaluation exhaustion stops further point probes, not sound
   bound-only classification. Unrefined brackets retain `tolerance_met = false`.

Fits are attempted only near the top of the tree; the default fit depth is one.
Bounds are cheap-first by construction: constant/skip certificates precede
all fitting and exact evaluation. Unknown bounds cause bounded subdivision.
There is deliberately no speculative partial-sum lower bound and no automatic
scan fallback that could turn unresolved time into accepted time.

An accepted region is a certificate over its own closed extent. Regions share
endpoints; an unresolved gap is never merged away. Brackets are diagnostic and
can repeat across nested searches. They establish opposite-side endpoint
values, not unique roots, root counts, or continuity. The algorithm does not
claim to enumerate every zero or to represent isolated acceptable tangent
points as positive-duration periods.

The four contracts are independent:

| Contract | What this prototype establishes |
| --- | --- |
| Discovery coverage | Certified regions are conditional on sound providers; all remaining time stays explicitly unresolved, even if every probe agrees. |
| Timestamp precision | Each detected sign bracket carries its actual width and a separate 0.1 s default tolerance check. Continuity and uniqueness are not inferred. |
| Numerical error | Providers must enclose every authoritative floating-point output and establish defined evaluation throughout their interval. Point output, threshold and bounds must be finite. |
| Resource limits | Bound calls and exact calls have separate caps, plus depth/width limits. Exhaustion can reduce coverage but cannot create acceptance. |

Counts include exact calls, bound calls, fits, failed fits, polynomial candidates,
subdivisions, refinements, bisection fallbacks, unresolved regions, and peak
pending intervals. Timing separates bounds, fitting (including exact fitting
probes and polynomial root solving), and refinement. The timers instrument the
same parallel path being measured. Callback execution itself is not preemptible;
this is an operation-count budget, not a hard wall-clock deadline.

## Component-bound feasibility

| Component | Investigated model behavior | Admitted certificate / remaining obstacle |
| --- | --- | --- |
| Starlight | A target-static map lookup is cached once by `PreparedThresholdQuery`; query-time evaluation adds that scalar. | `[cached, cached]` is exact for the returned model scalar. No new arithmetic error is introduced by this enclosure. Map calibration/physical uncertainty is not a continuous-time numerical-error claim. |
| Zodiacal | Fixed J2000 ecliptic latitude, solar longitude, folded elongation, bilinear Leinert table plus constant corner patches, reddening, Young airmass and Noll extinction, sampled spectral integration. | No certificate yet. Table-cell extrema alone omit reddening and propagation. In the current extinction code `tau0 = ln(10^(-0.4 kaer))` is negative, and the fitted effective factors change sign; assuming transmission <= 1 would be unsound. Validated angular ranges, transcendental errors, all branches and sampled-domain integration must be enclosed together. |
| Airglow | PALACE local-solar month/hour cells, F10.7 correction, clamped zenith, configurable Van Rhijn or vertical-profile geometry, Noll scattering and sampled trapezoidal integration. | No certificate yet. Scattering can exceed one near zenith; a unit transmission ceiling is not valid. Geometry integration and user-supplied profiles need validated error/domain bounds. Outside night still validates altitude/F10.7/scale, so an unconditional zero shortcut would hide errors. |
| Moonlight | Apparent topocentric Moon/target geometry, phase/distance, model selection, horizon/zero-separation branches, scattering grids and wavelength integration. Jones retains positive finite spectral nodes and may return zero when too few survive. | Only the **prepared planning evaluator's Moon-down skip** is admitted. Within a window disjoint from every closed prepared Moon-visible period, it adds zero. Active Jones and K&S evaluation have no certified envelope here. Bounds must include phase, distance, geometry, scattering, spectral retention and numerical exceptions. |

`planning/bounds.rs` admits only a sum of cached Starlight and skipped Moonlight
(or the empty component sum). It refuses all other selected combinations.
Adding zero to the cached scalar preserves the authoritative result; there is
no unchecked floating-point sum of independent component bounds. A Moon-down
certificate is conditional on the **existing** Siderust event periods and the
current inclusive `contains_time` semantics. It is not an independent proof of
lunar altitude-event completeness or true-sky brightness.

Rejecting a selected non-negative sum early could eventually be useful, but it
also needs a proof that every skipped component is defined and non-negative
throughout the interval. A pointwise finite-value check cannot establish that.
This spike intentionally refuses that optimization.

## Regime/discontinuity audit

| Regime | Existing handling | Consequence for the prototype |
| --- | --- | --- |
| Sun ceiling, astronomical night | Siderust events; night endpoints and thirds are in the prepared partition. | Retained. Airglow night endpoints have strict phase membership. |
| Target visibility | Siderust altitude events at the caller's floor. | Retained. A zero-altitude component boundary needs separate attention if the caller clears/lowers this filter. |
| Moon visibility | Siderust geometric-horizon periods; inclusive membership in the prepared evaluator. | Retained. A Moon-down interval touching a visible endpoint is **not** certified zero. Closed overlap tests enforce this. |
| PALACE climatology | Local-mean-solar hourly boundaries during astronomical night. | These already include local month/season changes; no duplicate seasonal partition is needed for PALACE. Legacy night thirds are not a replacement for hour boundaries. |
| F10.7 resolver | Stable UTC-date cache; issuance/retrieval dates bypass reuse. Resolution uses requested date, validity, issuance/retrieval, monthly evidence and fallback. | Cache volatility is **not** a time partition. UTC midnights and all relevant within-day issuance/retrieval instants must be partitioned before future Airglow fitting, including offset-normalized timestamps. No Airglow fits/certificates are enabled here. |
| Zodiacal table patches | Solar corner constant-fill boundaries can jump; bilinear-cell and reddening branches can introduce kinks. Extinction has separate dex branches. | No real Zodiacal fits/certificates are enabled here. A future smooth fit must isolate actual jump branches, not merely daily/Sun events. Wavelength-only branches are fixed during a time query. |
| Moonlight piecewise behavior | Horizon, zero separation/distance, positive-finite spectral filtering and insufficient retained nodes. | No active real Moonlight fits/certificates are enabled here. Moon-down endpoints remain unresolved. No smoothness or completeness claim follows. |

The benchmark does not fit dynamic real components because their additional
within-interval branches are not isolated yet. It measures their failure to
certify honestly; synthetic benchmarks measure fitting/refinement costs.
No new physical partition is added to production: there is
no new production interpolant, and incomplete modes do not return these
intervals as valid. Synthetic known jumps are partitioned before fitting;
Gauss fitting nodes are interior, while authoritative boundary validation
retains the actual value at the jump. Synthetic unknown black boxes can still
defeat every probe; their intervals remain unresolved.

## Validation

`planning/adaptive_validation.rs` exercises analytically bounded synthetic
signals using outward-rounded basic operations. It checks a crossing pair
inside one coarse cell, a sub-second hidden spike, a smooth narrow rational
peak, tangency, near-flat/equal thresholds, known jumps and their actual
boundary values, invalid/non-finite output, model error propagation,
resource exhaustion, adjacent floating-point times, deterministic seeded
quadratics and real prepared Moon-down certificates. The Starlight test uses
an explicitly experimental synthetic map, checks exact cached-value reuse
across a UTC year boundary, and refuses a selected dynamic remainder.

Probing certificates in tests is additional diagnostic evidence; the enclosure
formula/provider argument is the coverage justification. Fine scans are never
used as mathematical oracles. The jump test deliberately retains uncertainty
on the side whose closed endpoint contains the changed value.

```bash
cargo fmt --all --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --lib --bins --tests --examples --all-features --locked -- -D warnings
cargo test --release --locked -p nsb --lib planning::adaptive_validation
```

The benchmark test is explicitly ignored in ordinary tests because it is a
long performance experiment. It is executed separately in release mode; it
does not replace or skip any existing scientific test.

## Performance methodology and results

The reproducible harness is `scripts/benchmark-adaptive-threshold.sh`. Optional
`NSB_ADAPTIVE_FILTER` selects a scenario substring; `NSB_ADAPTIVE_REPEATS` defaults
to three (minimum three). The full matrix covers CTAO-N/S; day/week/month/year;
each selected component and production `ALL`; annual 1/10/100 targets sharing a
context; bright/down Moon; near-crossing and far thresholds. Targets after the
seed vary RA/Dec deterministically. Starlight cases use a small, explicitly
experimental map, because the production-default composition currently omits
unapproved bundled Starlight.

Both algorithms run in the **same release test binary** against the same
evaluator, inputs, dependency versions, and reused context, with two Rayon
threads. Both use identical target preparation and parallel physical-window
orchestration. Site preparation is timed separately and excluded from both
search measurements; final UTC conversion, global coalescing and serialization
are excluded. The baseline invokes the unchanged authoritative 600 s scanner
directly and counts its exact calls. These are threshold-core measurements,
not a claim that the entire public CLI has that latency. Both variants warm
up once; timed execution alternates their order. Three repetitions provide
medians/ranges, not reliable p95 estimates. Bound/fit/refinement CPU timers sum
across workers and are not additive wall-clock stages.

Per physical interval the experiment caps bound calls at 128 and exact calls
at 128. These are **experimental budget choices**, not resolution/completeness
guarantees or production optimization targets. The default engine tests use
larger independent budgets. Whole-query resource use scales with the number
of physical windows; a production API would need a query-wide budget policy.

Environment: Intel Core Ultra 9 185H, Linux 7.0.0-38-generic x86_64, rustc
1.97.1 (8bab26f4f 2026-07-14), locked dependency versions, release optimization,
`CARGO_BUILD_JOBS=2`, `RAYON_NUM_THREADS=2`. The unmodified annual CTAO-S CLI
command from `performance.md` measured 3.550 / 3.380 / 3.393 s (median 3.393 s)
and 10,360–10,664 KiB process maximum RSS. This is a fresh baseline, not a
comparison with #161's different machine and older dependencies/data.

Measured matrix data and comparison table are recorded in
`adaptive-threshold-benchmarks.md` and `adaptive-threshold-benchmarks.jsonl`.
For dynamic components a faster incomplete answer is **not an equivalent
performance improvement**. Only constant Starlight comparisons have equivalent
threshold classification coverage. Moon-only partial certification may omit
otherwise usable observing time; `ALL`, Airglow and Zodiacal remain entirely
unresolved in the prototype. No full-component speedup is claimed.

## Integration and migration decision

**No-go for replacing the normal search or removing `sample_step`.** The
prototype demonstrates safe discovery/classification with supplied envelopes
and proves that candidate fits can miss a narrow event without causing false
acceptance. It does not provide a validated dynamic-component envelope.
Deploying it as the normal API would make essential workloads incomplete.

Next work required before closing #234:

1. Develop outward-rounded Zodiacal propagation/geometry envelopes, including
   actual branch behavior and defined-evaluation guarantees. Profile their
   cost before expanding to Airglow and both Moonlight models.
2. Partition resolver transitions and component jumps with one-sided boundary
   semantics. Validate upstream astronomical event coverage separately.
3. Improve certified coverage and budget fairness for nearly equal signals;
   consider stronger monotonicity/derivative proofs or certified polynomial
   errors where demonstrable. Benchmark strategies with/without fitting.
4. Validate production-quality completeness/incomplete-result semantics,
   context ownership, determinism, memory and total latency, including tail
   latencies. Do not identify fine-scan agreement with proof.
5. Review an additive explicit certified/incomplete planning entry point first.
   The current frozen `ThresholdQueryResult` must not silently change meaning.
   A separate result contract should expose certified periods and unresolved
   time; an API that requires complete coverage should return an explicit
   incomplete error. Neither should return a bare empty schedule on failure.
6. Only after evidence and compatibility review, deprecate the sampling builder
   and field in a suitable release; removing them requires the project's
   breaking-change/SemVer policy, API snapshot, Python/stub, CLI and example
   updates. Keep a clearly named discrete diagnostic path if useful. Migrate
   `nsb-service-ctao` in a coordinated downstream API version afterwards.
