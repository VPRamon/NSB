# Performance contract

Status: Current performance evidence and benchmark scope.
Audience: Maintainers and reviewers of runtime or benchmark changes.
Scope: Evaluator reuse boundaries, long-horizon threshold searches, benchmark
workloads, and performance-review rules.
Non-goals: This document is not a scientific-validation substitute and does
not set portable timing thresholds for other machines.

`NsbEvaluator` owns immutable parsed component data. `SiteWindowContext` is the
target-independent reuse boundary for scheduler workloads at one site and UTC
interval. It contains astronomical nights, Sun-filter periods, conservative
Moon visibility, Airglow phase intervals and atmosphere, and a query-local
solar-activity cache. A context may only be used with its originating evaluator
and matching site, UTC window, components, and Sun filter. Target,
target-altitude floor, radiance threshold, and sample step may vary.

## Issue 160 baseline

Baseline and final measurements were made on 2026-09-14 on the same machine:

```text
CPU: AMD Ryzen 7 5700X, 8 cores / 16 threads, 32 MiB L3
OS: Linux 7.0.0-31-generic x86_64
Rust: rustc 1.97.0 (2d8144b78 2026-07-07)
Cargo: cargo 1.97.0 (c980f4866 2026-06-30)
Baseline commit: d228a85a02cb25e8b0130f7d6d6edbee51e4489e
Build: CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release --locked -p nsb-cli --bin nsb
```

The representative command was:

```bash
./target/release/nsb \
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
  --step 600 \
  > /dev/null
```

Five independent baseline executions took 5.80, 5.86, 5.88, 5.89, and 5.93
seconds: median 5.88 seconds. Median user CPU time was 5.85 seconds, system CPU
time was 0.02 seconds, and maximum resident memory was about 59 MiB.

Hardware counters and a sampling flamegraph could not be collected in this
environment. `perf stat` and `perf record` were denied because
`/proc/sys/kernel/perf_event_paranoid` is 4 and the process lacks
`CAP_PERFMON`; no supported userspace sampler was installed. Cycles,
instructions, IPC, cache misses, and branch misses are therefore unavailable,
not estimated. Compile-time-gated call counts and phase timers were added as
the reproducible profiling substitute. The diagnostics feature remains
sequential so thread-local counts are deterministic and imposes no counter or
clock overhead on production builds.

| Baseline phase or counter | Value |
| --- | ---: |
| Evaluator construction | 178.96 ms |
| Threshold preparation | 2.992 s |
| Astronomical-night preparation | 1.318 s |
| Target visibility | 164.59 ms |
| Moon visibility | 1.509 s |
| Threshold search | 2.830 s |
| Exact integrated evaluations | 5,965 |
| Zodiacal / Airglow / Moonlight evaluations | 5,965 / 5,700 / 2,112 |
| Candidate / adaptive / accepted-smooth intervals | 268 / 573 / 1,145 |
| Scan fallbacks | 124 |
| Crossings / crossing-refinement evaluations | 220 / 2,086 |

The data identified two bottlenecks. Site/time preparation repeated precise
solar and lunar ephemerides, while exact samples repeated Moon geometry,
spectral integration, solar-activity resolution, and coordinate transforms.
Starlight also parsed a large immutable CSV at every evaluator construction.

## Final architecture

- The validated bundled starlight map is converted to a compact binary asset at
  build time. Runtime loading checks its header and preserves the manifest and
  source-checksum contract. Startup fell from about 179 ms to about 8 ms.
- Solar events use a compact Meeus-style signal for six-hour discovery, then
  precise Siderust altitude samples polish and validate every boundary.
  Grazing, tangent, or failed brackets fall back to Siderust's full event
  search. Representative, equatorial, and polar tests bound retained event
  boundaries to one second against the precise search.
- Target visibility uses cheap horizontal-coordinate discovery at four-hour
  spacing. Every boundary is bracketed and refined with Siderust's precise
  altitude. Ordinary, never-visible, and circumpolar results agree with a
  ten-minute precise scan to two seconds.
- Astronomical-night search begins with a two-day expansion and grows only when
  a relevant night is clipped, avoiding unconditional long event searches while
  preserving continuous polar-night behavior.
- Threshold Airglow reuses a prepared model, evaluates the integrated scalar
  without allocating a spectrum, uses precomputed night phases, and caches
  resolved F10.7 by stable UTC date. Dates containing forecast issue or
  retrieval transitions bypass the cache. A year-spanning test compares cached
  values with full resolution, including transition dates.
- Exact Moon geometry obtains the precise lunar position once and derives
  topocentric position and phase from it. A reduced Meeus model is used only for
  discovery, inside a conservative -5 degree Moon-altitude envelope. Exact
  Moonlight remains authoritative inside retained intervals.
- Reduced Moon discovery was measured over two sites, two targets, and a year.
  Maximum observed errors were 1.5764 degrees in Moon zenith, 1.6723 degrees in
  Moon-target separation, 0.2829 degrees in phase angle, and 6,575.5 km in
  distance. These are discovery envelopes, not accepted-output tolerances.
- Zodiacal discovery uses a compact J2000 solar longitude whose maximum measured
  2026 error is below 0.02 degree. Exact output remains the published
  VSOP87/Siderust-based calculation.
- Adaptive search first splits at Sun, target, Airglow-phase, and conservative
  Moon boundaries. Cheap signals guide subdivision. A smooth interval is
  accepted only after exact endpoint and midpoint values agree and clear an
  eight-times-curvature margin. Terminal pairs and every reported crossing use
  authoritative values; ambiguous regions use exact scan fallback. Independent
  physical intervals run in parallel with local evaluation caches and ordered,
  deterministic collection.
- `SiteWindowContext` shares site/year work without process-wide mutable
  scientific caches. One-year/all-component context creation takes 45.95 ms on
  this machine. Multi-target measurements exclude this setup and include target
  visibility, target-static starlight, and threshold search.

A Chebyshev-first integrated-NSB fit and a precise lunar Chebyshev cache were
investigated but not retained. Exact-node validation plus physical boundaries
did not beat component-specific discovery, and lunar cache construction became
neutral once reduced discovery removed it from preparation. The retained
safeguarded Illinois/secant solver reduced refinement calls without changing
the one-second contract.

The principal measured checkpoints were:

| Change | Before | After | Scientific change |
| --- | ---: | ---: | --- |
| Bundled starlight construction | 178.96 ms | about 8 ms | None; packed values and source checksum are validated. |
| Exact Jones Moon point geometry | 854–859 µs | 500–504 µs | None within floating-point tolerance; the same precise ephemeris state is reused. |
| Site/time threshold preparation | 2.992 s | 66.26 ms diagnostics / 45.95 ms Criterion context | Exact roots are polished or use the precise fallback. |
| Crossing refinement calls | 2,086 | 1,449 | None; safeguarded interpolation retains the one-second stopping tolerance. |
| Full sequential optimized search | 5.88 s baseline | 2.23 s including preparation | Every smooth classification and crossing remains exact-validated. |
| Full production search | 5.88 s baseline | 0.36 s | Same model; independent physical windows execute in parallel. |

## Final measurements

Five final representative executions took 0.35, 0.36, 0.36, 0.36, and 0.37
seconds: median 0.36 seconds, median user CPU time 4.70 seconds, and about 32 MiB
maximum resident memory. This is a 16.3x wall-time speedup with 140 ms margin to
the required 500 ms target.

| One-year exact CLI workload | Baseline | Final | Speedup |
| --- | ---: | ---: | ---: |
| All components | 5.88 s | 0.36 s | 16.3x |
| Zodiacal only | 1.08 s | 0.07 s | 15.4x |
| Starlight only | 0.98 s | 0.06 s | 16.3x |
| Airglow only | 1.76 s | 0.07 s | 25.1x |
| Moonlight only | 3.51 s | 0.19 s | 18.5x |

The final sequential diagnostics build reports 5,832 exact integrated
evaluations (2.2% fewer than baseline) and 1,449 crossing-refinement evaluations
(30.5% fewer), with 5,567 Airglow and 2,663 Moonlight calls. Preparation is
66.26 ms and search is 2.165 s. The modest exact-call reduction shows that most
speedup comes from removing repeated preparation, reducing authoritative-sample
cost, and parallelizing independent intervals—not weakening the model.

The mandatory dedicated Criterion run used 0.2 s warm-up, a 1 s measurement
target, 10 samples, and no plots:

```text
threshold_window_duration_component/all/1y
time: [353.94 ms 357.23 ms 360.36 ms]
```

The complete short-run duration/component matrix was:

| Components | 1 day | 1 week | 1 month | 1 year |
| --- | ---: | ---: | ---: | ---: |
| All | 3.779 ms | 14.787 ms | 53.298 ms | 359.87 ms |
| Airglow | 1.009 ms | 2.223 ms | 6.084 ms | 59.886 ms |
| Moonlight | 2.854 ms | 32.537 ms | 40.775 ms | 175.40 ms |
| Zodiacal | 1.031 ms | 1.961 ms | 6.161 ms | 62.466 ms |

The original Criterion cases remain as like-for-like regression workloads:

| Existing all-component workload | Baseline | Final | Speedup |
| --- | ---: | ---: | ---: |
| One day | 831.20 ms | 7.685 ms | 108.2x |
| One week | 910.51 ms | 14.851 ms | 61.3x |
| One month | 1.3875 s | 45.986 ms | 30.2x |

Physical-regime medians were 4.124 ms for Moon-down, 15.386 ms for bright Moon,
1.814 ms for a never-visible target, 148.73 ms for a 30-day continuous Arctic
night, 52.393 ms for an ordinary 30-day target, and 48.351 ms near a crossing.

For 100 deterministic targets spanning declinations from -60 to +60 degrees
over the same CTAO-S year, the reusable-context benchmark measured:

| Targets | Baseline without shared context | Final with shared context | Speedup |
| ---: | ---: | ---: | ---: |
| 1 | 9.221 s | 576.76 ms | 16.0x |
| 10 | 76.845 s | 4.260 s | 18.0x |
| 100 | 705.46 s | 32.590 s | 21.6x |

The one-target row uses the first diverse target, not the representative target
from the hard objective. Baseline values are collected by a one-shot harness
using one evaluator but no shared context; final values are Criterion medians.

## Benchmark commands and coverage

Routine gates compile the benchmark and run its short smoke query, but do not
perform full Criterion measurement:

```bash
cargo test --workspace --all-targets --locked
```

Run the complete suite with:

```bash
cargo bench -p nsb --bench threshold_window
```

It includes 1 day/week/month/year for all, Airglow, Moonlight, and zodiacal
selections; the original regression cases; Moon-down, bright-Moon,
never-visible, long-night, ordinary, and near-crossing regimes; one-year context
preparation; and 1/10/100-target reuse. Short review runs may use:

```bash
cargo bench -p nsb --bench threshold_window FILTER -- \
  --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10 --noplot
```

The separate Airglow geometry microbenchmark remains:

```bash
cargo bench -p nsb --bench airglow_geometry
```

A short 2026-09-02 review run of that target used an Intel Core Ultra 9 185H,
Linux 7.0, Rust 1.97.1, locked dependencies, 0.1 s warm-up, 0.2 s measurement,
and 10 samples. It reported 8.96–9.12 ns for Van Rhijn, 1.64–1.66 µs and
3.14–3.19 µs for the 64- and 128-substep vertical profiles, 13.70–13.94 ms for
complete default Van Rhijn evaluation, and 13.53–13.63 ms for complete
vertical-profile evaluation. These are retained historical review values, not
portable pass/fail thresholds.

Performance changes must preserve exact component outputs, validate every
accepted threshold classification and crossing with authoritative values, and
retain the documented one-second boundary-refinement contract.
