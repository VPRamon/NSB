# Adaptive search scientific validation

The regression suite tests production discovery and strict classification, with
analytic functions, authoritative physical evaluations and independent local
references. It contains no historical certificate-feasibility engine. A compact
test-only discrete reference retains the original grid and Illinois crossing
rules for numerical comparisons and scan600 timing. Its separate uncached
radiance path also checks exact astronomical cache equivalence. Historical full
implementations remain available in Git.

## Coverage and interpretation

The 204-scenario production matrix covers CTAO-N/S, both Moonlight models,
1/7/30/365 days, 1/10/100 targets, individual components, all 16 masks, actual
`ALL`, explicit four-component composition, near/far Moon geometry and Moon-down
certificates. The historical matrix used a synthetic Starlight benchmark fixture.
That fixture is not the bundled, validated production Starlight asset
available in current builds. Composed thresholds add the cached scalar
so dynamic components still cross; pure Starlight retains its nominal
benchmark threshold.

The accepted 204-case optimization baseline preserves scientific outputs and
discovery counters. The corpus includes 105 locally confirmed rejected excursions and 13
accepted islands missed by the fine grid (scenario occurrences, not distinct
astronomical events), with zero unexplained disagreements or unresolved output.

The final engineering audit reruns all 204 cases against that baseline. All
complete, with the same 105/13 confirmed reference misses and no unexplained
disagreements, unresolved time or budget exhaustion. Two hundred cases retain
their period counts. Four North annual batch cases gain one occurrence each of
the same physically accepted window after replacing the incorrect J2000-to-ICRS
retag with an upstream frame transformation. For native J2000 target
RA 51.17192°, Dec −35°, the restored 2026-10-21 period runs from
02:32:57.548651114 to 04:42:01.325272099 UTC (7743.776621 s) in both lunar models.
The permanent full-year regression independently checks target/Sun point
altitudes and uncached authoritative radiance at one-second interior samples.
This independent check matters because the fine matrix shares eligibility
preparation. No physical formula or discovery tolerance changed. Upstream event
enumeration remains part of the conditional discovery contract; this observed
correction does not establish completeness for every possible annual geometry.

The original per-run timing records, counters, CLI comparisons and
boundary-corpus differences are preserved in
[the historical engineering audit](https://github.com/VPRamon/NSB/blob/421cf9db5ab08ffe158120193fa8af40f2ea642b/docs/specifications/adaptive-acceptance/engineering-audit.json).
They are no longer copied into the active documentation tree. The
regression tests, not those historical measurement files, are the
current executable acceptance criteria.

Every complete adaptive schedule is checked against independent authoritative
30-second scans for single targets and 120-second scans for batches. Global
matched-boundary tolerance remains 0.533 s: the reference's 0.432 s half bracket,
adaptive's 0.1 s accepted-side bracket and 0.001 s representation allowance.
Unexplained topology or boundary differences fail before a timing row is emitted.
Every timed output must exactly equal its validated warmup output.

A discrete reference can itself miss an event. Additional rejected excursions or
accepted islands require an independent local scan of at most 10 ms and both
boundaries within 0.106 s. Accepted islands also require authoritative interior
acceptance and eligibility clipping. A reference singleton jump artifact can be
excluded only after checking the physical edge and five rejected interior points;
coarse endpoint classification is checked just inside the edge. These explicit
conditions do not permit a general duration filter or relaxed boundary tolerance.

## Essential regressions

- Narrow double crossings, grazing extrema, tangencies, flat equality, jumps,
  and the smooth opposite-aliasing pulse visible to scan600 but missed by the
  earlier insufficiently validated fit.
- The approximately 53 ms Zodiacal rejection and 2.4006 s PALACE/Jones rejection,
  independently resolved at 1 ms; the 87.6283 s twilight rejection independently
  resolved at 10 ms.
- Accepted islands missed by scan120: approximately 7.1992 s (South, both lunar
  models), 9.8427 s (North K&S), and 13.7884 s (North Jones), with 1 ms references.
  The original measurements are
[archived with the acceptance corpus](https://github.com/VPRamon/NSB/blob/421cf9db5ab08ffe158120193fa8af40f2ea642b/docs/specifications/adaptive-acceptance/accepted-islands.json).
- UTC midnight, adjacent split-time instants, Moon visibility inclusivity,
  accepted-side min/max bands, invalid model inputs, resource exhaustion and
  deterministic schedules/errors across Rayon worker counts.
- Native endpoints separated below a scalar MJD ULP remain distinct physical
  partitions and coalesce in precise temporal order; target eligibility matches
  the upstream J2000-to-ICRS transformation.
- Native leap-second windows, fallible CLI UTC history parsing, cache poisoning,
  and incompatible observer reuse have focused regressions.
- Invalid point/planning coordinates and filters fail before eligibility can
  turn them into an apparently valid empty schedule. CLI ICRS coordinates round
  trip through the native target frame with the actual frame bias applied.
- Invalid lunar states fail for both models; the reused geocentric phase kernel
  matches all upstream phase fields hourly through a full lunation.
- Exact cached/uncached component equality across both sites, both Moon models,
  all masks and repeated reads; distinct sub-microsecond keys and bounded capacity.
- A context populated under an earlier Tempoch time-data identity is rejected
  through the public API; preparing a fresh context restores valid evaluation.
  This regression simulates stale identity without mutating global time data.

The 64-case seven-day boundary corpus validates all masks, sites and Moon models.
Its verified baseline has 716 accepted native UTC endpoints, 712 paired fine
boundaries, maximum matched error 0.41529231 s, two independently confirmed
reference misses, zero unpaired boundaries and zero rejected native endpoints.
The [historical boundary records](https://github.com/VPRamon/NSB/blob/421cf9db5ab08ffe158120193fa8af40f2ea642b/docs/specifications/adaptive-acceptance/boundary-accuracy.jsonl)
and [summary](https://github.com/VPRamon/NSB/blob/421cf9db5ab08ffe158120193fa8af40f2ea642b/docs/specifications/adaptive-acceptance/boundary-accuracy-summary.json)
remain available in Git history, while the corresponding regressions remain executable.
A fresh engineering-audit run retains all 716/712 counts, zero rejected/unpaired
endpoints, and the same maximum error; ten error records change by at most
0.000034662 s after the frame/precision corrections. The baseline goldens remain
unchanged. This is observed corpus accuracy, not a global error theorem.

The Siderust 0.13-to-0.14 migration produced 67 identical dependency-reference
records (64 physical cases plus three temporal round trips). The immutable
[0.13 export](https://github.com/VPRamon/NSB/blob/954817f71f404f21fc9ff9fdd37c870506532c89/docs/specifications/adaptive-acceptance/dependency-A.jsonl)
and [0.14 export](https://github.com/VPRamon/NSB/blob/954817f71f404f21fc9ff9fdd37c870506532c89/docs/specifications/adaptive-acceptance/dependency-B.jsonl)
remain accessible in Git instead of duplicating 1.3 MB in the final tree.

Python result construction owns projection validation for all nine search
variants. Rust tests inject native submicrosecond and subnanosecond endpoints,
including an 800 ns collapse, and follow valid periods through the actual result
getter and Tempoch Python interop. They cover inward rounding, UTC midnight,
leap-second-adjacent instants, unsupported leap labels and years 0/10000.
Installed-wheel tests exercise ordinary, adaptive, certified and joint-band
outputs on four civil boundary windows, asserting `native_start <= start < end
<= native_end` at exported precision.

The near-extremum Zodiacal fixtures retain the exact reviewed radiance thresholds.
Their four original authoritative medians are frozen in the test instead of being
recomputed from implementation-dependent partition midpoints. The approximately
53 ms excursion still requires independent 1 ms confirmation; no threshold or
boundary tolerance is relaxed. Frame corrections intentionally alter eligibility
edges (0.614 ms in the focused fixture); they do not change component radiance formulas.
The engineering audit compares those changes with the accepted baseline and
reruns the complete scientific corpus rather than replacing its goldens silently.

## Reproduction

```bash
cargo test --release --locked -p nsb --lib planning::discovery_tests
cargo test --release --locked -p nsb --lib planning::strict_tests
cargo test --release --locked -p nsb --lib physical_boundary_accuracy_reference -- --ignored --nocapture
mkdir -p target
set -o pipefail
CARGO_BUILD_JOBS=2 RAYON_NUM_THREADS=2 NSB_DISCOVERY_REPEATS=3 \
  cargo test --release --locked -p nsb --lib complete_adaptive_schedule_matrix \
  -- --ignored --nocapture --test-threads=1 | tee target/adaptive-matrix.log
```

Run native Rust, installed-wheel Python and CLI suites as well; their inward
projection contracts differ in representable precision. Use the [current performance contract](performance.md) for benchmark
commands. The older three-way study is [archived in Git](https://github.com/VPRamon/NSB/blob/421cf9db5ab08ffe158120193fa8af40f2ea642b/docs/specifications/adaptive-performance.md).
