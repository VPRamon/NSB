# Performance contract

Status: Current performance guidance and benchmark scope.
Audience: Maintainers and reviewers of runtime or benchmark changes.
Scope: Evaluator reuse boundaries, benchmark workloads, and performance review
rules.
Non-goals: This document is not a scientific validation substitute and does not
set portable pass/fail timing thresholds.

`NsbEvaluator` is the reuse boundary for production workloads.

- Zodiacal tables, the starlight map, and airglow calibration are parsed once.
- Experimental or validated external starlight maps are validated/parsed before
  evaluator construction and cloned once into the evaluator, never per lookup.
- Airglow 300–650 nm integrals, uncertainty integrals, and B/V samples are
  precomputed once; point evaluation performs scalar scaling without temporary
  spectral vectors.
- Threshold searches cache target-static starlight, pre-filter with Sun/target
  events, prepare astronomical-night and Moon-visibility intervals once per
  query, split candidate windows at physical regime boundaries, use adaptive
  exact-sample search on smooth intervals, and refine bracketed crossings to
  about one second rather than microsecond precision.
- During threshold searches, airglow time-of-night bins are derived from the
  prepared astronomical-night context instead of recomputing solar event periods
  for every sampled time. Point evaluation keeps the standalone exact lookup.
- Moonlight is evaluated only inside query-level Moon-up periods. Inside those
  periods the exact moonlight model remains authoritative.
- Short or numerically unclear threshold intervals fall back locally to the
  bounded scan path; no accepted threshold crossing is reported without exact
  component evaluations and bracketed refinement.
- Full point results allocate only the selected component report vector and
  owned metadata required by the public result.

## Benchmark Commands

Routine test gates compile the benchmark target and run a short smoke query, but
do not execute the full Criterion workload:

```bash
cargo test --workspace --all-targets --locked
```

Run the real benchmark suite manually or in scheduled performance jobs:

```bash
cargo bench -p nsb --bench threshold_window
cargo bench -p nsb --bench airglow_geometry
```

For review runs that need a shorter wall-clock time while preserving Criterion
measurement, use explicit measurement settings and record them with the result:

```bash
cargo bench -p nsb --bench threshold_window 'threshold_window/(1d|1w|1mo)' -- \
  --warm-up-time 0.1 \
  --measurement-time 0.2 \
  --sample-size 10 \
  --noplot
```

Use the same command, machine, Rust toolchain, and dependency lockfile for
before/after comparisons.

## Benchmark Coverage

The `threshold_window` Criterion target covers zodiacal, airglow, moonlight,
default composition, experimental starlight lookup, and 1-day/1-week/1-month
window searches. It also includes Moon-low, Moon-bright, target-never-visible,
and long astronomical-night cases so interval reuse remains measurable.

The benchmark harness reports wall-clock Criterion timings. Event-search counts
are code-derived for review: a threshold query performs one target-visibility
event search when a target floor is configured, one Moon-up search when
moonlight is selected, and either one reusable astronomical-night search for the
default airglow and Sun filter or one separate Sun-filter search for a different
Sun ceiling. There is no per-sample solar event search in threshold sampling;
library tests enforce that path. Exact integrated-evaluation and crossing counts
are not emitted by the Criterion harness, but the adaptive search tests compare
against the scan fallback and verify reduced exact sample calls on clear smooth
intervals.

The `airglow_geometry` Criterion target separately measures the legacy-fast Van
Rhijn factor, direct spherical vertical-profile integration with 64 and 128
substeps per altitude interval, and normal Airglow evaluations using each model:

```bash
cargo bench -p nsb --bench airglow_geometry -- \
  --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10 --noplot
```

The direct integrator is the reference and production profile path. No geometry
cache or interpolation is currently used; refinement tests protect convergence
if future measurements justify an acceleration layer.

A short review run on 2026-09-02 used an Intel Core Ultra 9 185H, Linux 7.0,
Rust 1.97.1, locked dependencies, and the command above. Criterion reported:

| Airglow geometry workload | Measured interval |
|---|---:|
| Van Rhijn factor | 8.96-9.12 ns |
| Vertical profile, 64 substeps/interval | 1.64-1.66 us |
| Vertical profile, 128 substeps/interval | 3.14-3.19 us |
| Complete default Van Rhijn evaluation | 13.70-13.94 ms |
| Complete vertical-profile evaluation | 13.53-13.63 ms |

These short-run values are review evidence, not portable pass/fail thresholds.

Representative release-build measurements from the 2026-06-24 development
container are baselines, not portable pass/fail thresholds:

| Workload | Median-scale time |
|---|---:|
| Zodiacal point | 108 µs |
| Airglow point | 17.3 ms |
| Moonlight point | 0.87 ms |
| Full default point | 18.1 ms |
| Experimental starlight lookup/composition | 503 ns |
| One-day default threshold window | 0.95 s |

## Threshold Optimization Evidence

The threshold optimization was measured on 2026-07-03 on:

```text
CPU: Intel Core Ultra 9 185H
OS: Linux 6.17.0-35-generic x86_64
Rust: rustc 1.93.1
Profile: cargo bench, optimized, locked dependencies
Command: cargo bench -p nsb --bench threshold_window -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10 --noplot
```

| Workload | Baseline median | Optimized median | Speedup | Notes |
| --- | ---: | ---: | ---: | --- |
| Threshold window, 1 day | 0.935 s | 0.580 s | 1.61x | Default components. |
| Threshold window, 1 week | 6.95 s | 0.662 s | 10.5x | Default components. |
| Threshold window, 30 days | 20.0 s | 1.03 s | 19.4x | Default components. |

Additional optimized medians from the same run:

| Workload | Optimized median | Notes |
| --- | ---: | --- |
| Moon-low window | 29.3 ms | Moonlight-only case; Moon-down samples are skipped by precomputed Moon-up intervals. |
| Moon-bright window | 66.8 ms | Moonlight-only case with bright Moon geometry. |
| Target never visible | 578 ms | Candidate windows are empty after target-altitude filtering. |
| Long astronomical night | 271 ms | Airglow-only high-latitude case using prepared night phase context. |

Hardware and virtualization affect absolute values; compare changes on the same
runner.

Performance changes must preserve component outputs within validation tolerance
and window boundaries within the documented one-second refinement contract.
