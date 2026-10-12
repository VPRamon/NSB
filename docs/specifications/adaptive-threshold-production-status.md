# Adaptive threshold search: production integration status

PR [#235](https://github.com/VPRamon/NSB/pull/235), continuing
[#234](https://github.com/VPRamon/NSB/issues/234).
**Partial production integration; the all-component acceptance criteria remain unmet.**
The PR must stay Draft. The [initial audit and measurements](adaptive-threshold-search.md)
are retained as historical evidence, not measurements of this revision.

## Implemented production behavior

The discovery engine and bound providers now compile normally in `nsb`.
`cheby` is a direct dependency at the already-locked version, with no new crate
or dependency-version change. Siderust still owns all astronomical events.

The normal planning methods first classify existing physical windows using
sound certificates. Cached Starlight and skipped Moonlight can avoid exact
evaluations. Uncertified regions take the existing **discrete** scanner path,
with its existing caller-controlled resolution and limitations. This retains
the supported contract; it is explicitly **not** the requested general
continuous-time adaptive replacement.

The additive methods `NsbEvaluator::periods_below_threshold_certified` and
`periods_below_threshold_certified_with_context` require every eligible region
to be classified. They never use a uniform scan fallback. Success contains
only certified observing periods, ordered/coalesced and converted to UTC using
the endpoint conversion with a strict containment guard. Native UTC,
CLI nanosecond civil output, and Python microsecond-truncated civil output must
all map back inside the closed TT certificate. Civil years outside Python’s 1–9999 range, unrepresentable leap labels or
outward-rounded endpoints are adjusted inward on a microsecond grid when
representable within 0.1 s. At most 64 candidates per endpoint are checked;
if none passes, `Unsupported` is returned and no schedule is emitted. Classification
covers all eligible TT time; exported boundaries may omit at most 0.1 s per
endpoint. This
is a representation check against the prepared TT model, not a bound on
astronomical time-scale or ephemeris errors. An entirely unacceptable window
can legitimately return an empty result. Any unresolved eligible time instead
returns `NsbError::Unsupported` with reason counts and budget flags.

Python exposes the same two methods and raises `UnsupportedError` on incomplete
coverage. CLI `window --certified` selects this strict contract and conflicts
with an explicit `--step`. It exits unsuccessfully without writing a schedule
when certification is incomplete. Existing Rust/Python methods and CLI defaults
retain their sampled contract. No `nsb-service-ctao` changes are made.

## Component status

| Selected component | Authoritative / sampled planning | Strict interval certification |
| --- | --- | --- |
| Starlight | Supported with a configured admitted or experimental map. | Exact cached scalar; complete without repeated radiance evaluations. |
| Zodiacal | Existing Leinert/Noll behavior retained. | Unsupported active temporal envelope; explicit incomplete error. |
| Airglow | Existing PALACE, F10.7, geometry and atmosphere behavior retained. | Unsupported active temporal envelope; no zero shortcut bypasses validation. |
| K&S Moonlight | Existing analytic scientific model retained. | Prepared Moon-down skip only; active intervals are unsupported. |
| Jones Moonlight | Existing spectral scientific model retained. | Prepared Moon-down skip only; active intervals are unsupported. |
| `ComponentMask::ALL` | Usable existing discrete schedules, with certificate fast paths where possible. | Realistic nonempty eligible windows remain incomplete. **Not production-ready certified ALL.** |

Every mask is covered by regression tests with both lunar models. Agreement
with the previous scanner is regression evidence, not a completeness proof.
In this build, the frozen `ComponentMask::ALL` equals Zodiacal | Airglow |
Moonlight (bits 13), not all defined bits. Its existing policy includes Starlight
only when an admitted production map is bundled; otherwise Starlight is opt-in.
All 16 masks are tested, including bits 15 (all four physical components), using
an explicitly configured experimental Starlight fixture. The benchmark's `all`
labels retain the frozen bits-13 meaning; Starlight has separate rows.

## Scientific and resource contracts

Only closed-interval enclosures classify time: upper <= threshold accepts;
lower > threshold rejects. Providers must cover the authoritative floating-point
outputs and defined evaluation, not only ideal formulas or sampled points.
Starlight uses the exact cached scalar. Moon-down is conditional on the existing
prepared inclusive Siderust visibility set, not an independent event-completeness
proof. The existing Sun/target eligibility partitions are also authoritative
inputs, not independently validated astronomical-event enclosures. Touching a
Moon-visible endpoint is never certified zero.

The provider now distinguishes a refinable enclosure, an inconclusive fixed
enclosure, a known regime boundary needing subdivision, and an unsupported
temporal envelope. Unsupported/fixed uncertainty stops in one classification;
it does not trigger fits, exact probes or a subdivision tree. Numerical failures
propagate as errors, separately from unsupported regions and budget exhaustion.
A wholly Moon-visible interval stops as unsupported. A window straddling a
visible endpoint may split to isolate a certifiable Moon-down interior.

One discovery invocation spans **all physical windows in a target query**.
Its enclosure and exact-evaluation caps never reset per physical window.
Breadth-first work gives root windows priority over deeper work; the final
regions are sorted deterministically. The production cap is 2,048 enclosure
calls per target query. Real fits are disabled and the exact-call allowance is
zero, because active model branches are not enclosed or isolated for fitting.
The sampled compatibility path has its previous separate resource contract.
These caps govern discovery operations, not ephemeris preparation, batch-wide
wall time, or callback preemption.

The general internal engine retains Chebyshev candidate generation and exact
bisection. A fit reserves eleven probes; once that reservation is unavailable,
later fits stop. Actual exact-budget exhaustion is recorded independently.
No further prediction/refinement probes run at the cap, while bounds may still
classify other regions. A bracket records opposite-side endpoint values and
actual width/tolerance status; it asserts neither continuity nor a unique root.
An unresolved neighborhood is never promoted to acceptance because it is small.

## Why full certification is still blocked

The [component and transition audit](adaptive-threshold-search.md) remains
applicable. The newly inspected active formulas do not supply the missing proofs:

- Zodiacal needs temporal solar-longitude and apparent-altitude envelopes,
  conservative table/corner and reddening propagation, and enclosure of the
  actual piecewise logarithm/exponential extinction and spectral integration.
  Its implemented negative `tau0` means a unit transmission ceiling is unsound.
- Airglow needs validated apparent altitude, Van Rhijn or configurable vertical
  profile geometry, scattering, PALACE cell factors, and F10.7 resolver ranges
  with defined-evaluation guarantees. A stable-date cache is not a partition at
  every date/issuance/retrieval transition. Outside-night zero still validates
  inputs; skipping those checks would change error behavior.
- K&S needs temporal phase, separation and apparent-altitude enclosures. Bounds
  on its analytic scattering/illuminance factors alone do not bound their
  time-varying composition near a threshold or prove geometry evaluation valid.
- Jones additionally needs distance/phase envelopes, spectral scattering-grid
  and propagation bounds, and enclosure of the positive-finite node retention,
  insufficient-node and integration branches. Ignoring these branches changes
  the authoritative numerical signal.

The pinned Siderust APIs provide point evaluations and event periods, not
validated interval enclosures for these inputs. A separately validated interval
propagation layer or upstream enclosure/derivative support is still needed.
Observed finite differences, Chebyshev tails and fine scans do not supply that
missing proof. Portable rounding bounds for the implemented transcendental
pipeline must also be justified; an arbitrary floating-point padding is not
an error bound. This is an unimplemented proof obligation, not a claim that
certification is mathematically impossible.

Even after component envelopes exist, complete root isolation requires a
reviewed one-sided boundary/uncertainty contract. Current sign brackets cannot
turn unresolved crossing neighborhoods into complete certified coverage.

## Validation and performance evidence

The complete license check passes after adding the required SPDX/copyright
header to the summary script. New regressions cover immediate unsupported and
fixed-envelope termination, global budgets across disjoint windows, continued
bound-only classification after exact exhaustion, numerical provider errors,
all 16 masks with both lunar models, strict failure behavior, Python/context
reuse and CLI failure without a misleading empty schedule.

The follow-up matrix uses one budget per whole target query and includes both
lunar models. Its baseline remains the unchanged authoritative 600-second
scanner; incomplete certified-only results are never speedup claims. Historical
per-window-budget prototype data remain in the original 55-scenario artifacts.
Fresh core measurements and end-to-end results are recorded separately with
their source revision, repetitions and resource settings. The core certified-only
harness executes no uniform fallback: its `fallback_intervals` field counts
retained unresolved regions, not completed fallback scans. The end-to-end
compatibility cases do execute the existing scanner on such regions.

### Recorded follow-up measurements

- [Fresh 83-case core matrix and three-variant comparison](adaptive-threshold-review-benchmarks.md): one warmup and three timed repetitions per variant, two Rayon threads, shared site context, both lunar models. Nine complete classifications and 74 incomplete outcomes. The run took 58 min 4.18 s; observed test-process RSS was 13,056 KiB. Command-wide RSS is recorded separately and is not attributed to either algorithm.
- [End-to-end CLI comparison](adaptive-threshold-cli-benchmarks.md): six scenarios, twenty alternating repetitions per binary, byte-identical outputs, observed p90 and per-child RSS. Four annual default-ALL cases retain the sampled contract; two one-hour Moon-down cases complete through the strict API.
- [Bound-only synthetic comparison](adaptive-threshold-bounds-only-benchmarks.md): close-pair/tangent costs fall to 8.33/6.95 µs from fit-assisted 16.28/14.34 µs, with the same unresolved neighborhoods. The cheap uniform scan has different discovery coverage, so these are not equivalent-result speedups.

For southern/northern annual 100-target Jones workloads, the fresh uniform
medians are 207.43/185.65 s; certified-only discovery measures 3.96/3.71 s but
leaves every eligible ALL interval unresolved. Historical prototype bound counts
of 21,060,404/20,813,506 fall to 153,687/149,390. This verifies elimination of
useless subdivision, **not** usable certified ALL performance. No material
production speedup or full-component performance acceptance is established.

Final local validation: 861 passed, zero failed, five ignored across 48 workspace
suites; 35 tests pass against the rebuilt abi3 wheel. Fifteen release adaptive
tests and the release UTC-projection regression pass. Formatting, full license
check and all-feature Clippy pass. Public API compatibility passes with exactly
two additions and no removed/changed items. The release benchmark is separately
executed despite being ignored by the normal suite. All 17 executed GitHub
checks pass on the implementation revision; the repository intentionally skips
coverage for Draft PRs. The final pushed documentation head is also checked.

## API and remaining acceptance criteria

The frozen API snapshot gains only the two additive Rust methods. No existing
signature, result field, query builder, component model or `sample_step` meaning
is removed. Python/stubs and the CLI expose the corresponding explicit mode.
Historical SemVer checks still deny removed/changed items. The new methods make
the stronger contract explicit instead of changing the meaning of the existing
result silently.

`sample_step` is unnecessary for strict discovery but remains required by the
sampled compatibility algorithm's resolution contract. Removing it from the
normal dynamic search is **not safe yet**. Full all-component adaptive
replacement, active K&S/Jones certification, usable certified ALL schedules,
crossing completeness, and production performance acceptance remain open.
Once validated envelopes and root isolation are available, an additive result
can expose certified regions, unresolved time and boundary brackets, followed
by a reviewed deprecation/migration of the sampled methods, CLI/Python options
and downstream CTAO service. The current PR does not close #234.
