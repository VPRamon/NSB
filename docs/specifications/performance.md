# Performance contract

Status: current correctness and performance evidence for long-horizon threshold
searches. Timings are review evidence, not portable pass/fail thresholds.

`NsbEvaluator` owns immutable parsed component data. `SiteWindowContext` is the
target-independent reuse boundary for workloads with one evaluator, site, UTC
window, component mask, and Sun filter. It holds the astronomical-night and
Airglow-phase intervals, Siderust Moon-visible intervals, the prepared Airglow
model, and a query-local solar-activity cache. Target, target-altitude floor,
radiance threshold, and sample step may vary. The context validates its owner
and invariant fields before every reuse.

## Correctness contract

NSB no longer uses reduced component signals to classify or discard radiance
intervals. Every candidate interval is scanned with the authoritative
integrated NSB model at the caller-selected `sample_step`; every bracketed
threshold crossing is refined with that same model by the safeguarded
secant/Illinois solver. Endpoint and midpoint agreement is not treated as proof
about an unsampled interior.

This is a discrete-resolution contract. An excursion narrower than
`sample_step` that lies wholly between authoritative samples can still be
missed. No general derivative or curvature bound exists for the combined model,
so the implementation does not claim continuous completeness. Callers needing
finer resolution must select a smaller step. This limitation is distinct from
the removed discovery pruning: a secondary approximation can no longer exclude
an interval before authoritative sampling.

Astronomical event finding belongs to Siderust:

- Sun filters and astronomical-night boundaries use Siderust's specialized
  solar altitude-event path, including its exact validation and fallback.
- Fixed-target visibility uses Siderust `event::altitude` for the ICRS
  direction. NSB has no four-hour target-altitude prefilter.
- Moon visibility uses Siderust `event::altitude` for the Moon at the geometric
  horizon. NSB has no reduced-lunar, fixed-cadence visibility guard.

NSB owns composition of these physical periods, Airglow phase semantics, exact
component evaluation, radiance scanning, crossing refinement, result
coalescing, and context reuse. Reduced lunar and zodiacal geometry remains only
in tests that quantify approximation error; it is not in the production window
search.

Production independently prepares Sun/Airglow and Moon state with one
`rayon::join`, then processes independent threshold windows in Rayon. Collection
order and final coalescing are deterministic. Each scan carries its previous
authoritative endpoint value forward, and crossing refinement receives both
bracket endpoint values directly; no per-window exact-evaluation map is needed.
The `window-search-diagnostics` feature deliberately uses the same scientific
search sequentially so thread-local counters and phase timings are stable.
Diagnostic wall time is therefore not production performance.

## Retained neutral optimizations

- The bundled production starlight map follows the same canonical CSV admission
  path as external production maps. It rechecks the sidecar against the actual
  CSV bytes and headers, parses the declared HEALPix geometry, validates every
  value and uncertainty, and recomputes flux-conservation and map diagnostics.
  The former build-packed shortcut was removed because it bypassed parts of
  this contract.
- Airglow threshold evaluation computes the integrated scalar without
  allocating a full spectrum. Broad equivalence tests cover three site
  profiles, two seasons, three night phases, four altitudes, and three F10.7
  values. Adjacent scalar-integration intervals reuse their shared endpoint.
- Resolved F10.7 values are cached by stable UTC date; issue/retrieval transition
  dates bypass unsafe reuse.
- A `SiteWindowContext` is reusable across many targets and thresholds. The CLI
  prepares it once for `--max-nsb` and reuses it for optional `--min-nsb`.
- Exact point evaluations reuse Moon ephemeris state. Instrumentation of the
  former per-window exact-evaluation map on the representative annual workload
  recorded 0 hits and 7,764 misses across 573 windows, so the map was removed.

## Measurement environment

Measurements were taken on 2026-09-14 on the same host used for the issue-160
baseline:

```text
CPU: AMD Ryzen 7 5700X, 8 cores / 16 threads, 32 MiB L3
OS: Linux 7.0.0-31-generic x86_64
Rust: rustc 1.97.0 (2d8144b78 2026-07-07)
Cargo: cargo 1.97.0 (c980f4866 2026-06-30)
Baseline commit: d228a85a02cb25e8b0130f7d6d6edbee51e4489e
Build: cargo build --release --locked -p nsb-cli --bin nsb
Review resource cap: CARGO_BUILD_JOBS=2, RAYON_NUM_THREADS=2, nice -n 10
```

The representative production command was:

```bash
RAYON_NUM_THREADS=2 nice -n 10 ./target/release/nsb \
  --format csv window \
  --start 2026-01-01T00:00:00Z \
  --end 2027-01-01T00:00:00Z \
  --site CTAO-S \
  --site-profile cta-south \
  --ra 83.6331 \
  --dec 22.0145 \
  --max-nsb 0.25 \
  --sun-altitude-max -18 \
  --target-altitude-min 20 \
  --step 600 >/dev/null
```

The latest 2026-09-19 review measurement used three resource-capped production
executions at commit `a12043c`, after removing the unused exact-evaluation
cache: 3.27, 3.29, and 3.13 seconds (median 3.27 seconds), with
5.19, 5.07, and 5.00 seconds user CPU time and about 58 MiB maximum resident
memory. The immediately preceding branch head measured 3.34, 2.89, and 2.99
seconds (median 2.99 seconds) under the same command and build profile; this
small three-run sample does not establish a statistically significant speedup.
The subsequent sampled-domain zodiacal correction only removes the accidental
constant edge additions and was not given a new benchmark campaign.
The original issue-160 implementation measured 5.88 seconds on this host, so
the correctness-first result retains a 1.80x wall-time improvement. The reviewed
but unsafe PR state measured about 0.36
seconds; it is recorded only as historical context and is not a valid scientific
performance target because its discovery paths could produce false negatives.

The dedicated Criterion run used production code, a 0.1 s warm-up, a requested
0.2 s measurement target, 10 samples, no plots, and the same two-thread cap:

```text
threshold_window_duration_component/all/1y
time: [3.5184 s 3.5306 s 3.5460 s]
```

## Workload results

The baseline column is the historical issue-160 measurement. Final values are
single resource-capped runs except for the three-run annual median above.

| One-year production workload | Baseline | Final | Baseline / final |
| --- | ---: | ---: | ---: |
| All components | 5.88 s | 3.27 s median | 1.80x |
| Zodiacal only | 1.08 s | 1.40 s | 0.77x |
| Starlight only | 0.98 s | 1.08 s | 0.91x |
| Airglow only | 1.76 s | 1.08 s | 1.63x |
| Moonlight only | 3.51 s | 3.85 s | 0.91x |
| All components, max plus min threshold | not recorded | 5.30 s | n/a |

Context/multitarget measurements use the deterministic target sequence defined
in `threshold_window.rs`. Preparation is excluded from each target row. To
avoid at least ten repetitions of the expensive 100-target case during the
resource-constrained review, final figures are one-shot production
measurements of the same benchmark body.

| Site/year workload | Historical PR measurement | Corrected final |
| --- | ---: | ---: |
| Context preparation only | 45.95 ms | 1.510 s |
| 1 target with shared context | 576.76 ms | 2.661 s |
| 10 targets with shared context | 4.260 s | 23.957 s |
| 100 targets with shared context | 32.590 s | 193.265 s |

The increase relative to the unsafe PR is expected: Siderust now performs
authoritative fixed-target visibility and the radiance model visits every
selected-resolution sample instead of accepting large intervals from a reduced
signal. Context reuse still avoids repeating the 1.510 s site/Moon/Sun setup.
For comparison, the pre-context one-shot baselines were 9.221, 76.845, and
705.46 seconds for 1, 10, and 100 targets respectively.

## Sequential diagnostics

One diagnostic run of the representative year reported the following. These
numbers explain work; its 5.031 s combined preparation/search time must not be
compared with production wall time.

| Diagnostic phase or counter | Value |
| --- | ---: |
| Evaluator construction | 187.37 ms |
| Threshold preparation | 2.516 s |
| Astronomical-night preparation | 660.84 ms |
| Target visibility | 166.68 ms |
| Moon visibility | 1.688 s |
| Threshold search | 2.515 s |
| Authoritative integrated evaluations | 7,459 |
| Zodiacal / Airglow / Moonlight evaluations | 7,459 / 7,194 / 3,283 |
| Candidate / authoritative-scan windows | 268 / 573 |
| Crossings / refinement evaluations | 220 / 1,414 |

Hardware counters remain unavailable because `perf_event_paranoid=4` and the
process lacks `CAP_PERFMON`; no cycles, IPC, cache-miss, or branch-miss values
are inferred.

## Benchmark and verification commands

```bash
cargo bench -p nsb --bench threshold_window
cargo bench -p nsb --bench threshold_window \
  threshold_window_duration_component/all/1y -- \
  --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10 --noplot
cargo bench -p nsb --bench airglow_geometry
```

The threshold benchmark covers 1 day/week/month/year by component, physical
Moon/visibility/long-night regimes, context preparation, and 1/10/100 targets
sharing a context. Normal CI compiles and smoke-runs benchmark binaries through
`cargo test --workspace --all-targets --locked`; it does not run the full
Criterion matrix.

Performance changes must preserve exact component outputs, keep astronomical
event ownership in Siderust, prevent approximation-based pruning, refine every
reported bracket with authoritative values, and document the selected sampling
resolution honestly.
