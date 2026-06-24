# Performance contract

`NsbEvaluator` is the reuse boundary for production workloads.

- Zodiacal tables, the starlight map, and airglow calibration are parsed once.
- Custom starlight maps are cloned once during evaluator construction, never per
  lookup.
- Airglow 300–650 nm integrals, uncertainty integrals, and B/V samples are
  precomputed once; point evaluation performs scalar scaling without temporary
  spectral vectors.
- Threshold searches cache target-static starlight, pre-filter with Sun/target
  events, evaluate only candidate intervals, and refine crossings to about one
  second rather than microsecond precision.
- Full point results allocate only the selected component report vector and
  owned metadata required by the public result.

The `threshold_window` Criterion target covers zodiacal, airglow, moonlight,
default composition, experimental starlight lookup, and 1-day/1-week/1-month
window searches. It runs in the scheduled/manual benchmark workflow, not normal
PR CI.

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

Hardware and virtualization affect absolute values; compare changes on the same
runner.

Performance changes must preserve component outputs within validation tolerance
and window boundaries within the documented one-second refinement contract.
