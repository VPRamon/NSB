# Adaptive threshold feasibility measurements

Measured 2026-10-12 against main `a3f6752`. See [methodology and limitations](adaptive-threshold-production-status.md). Raw repetitions/counters are in [JSONL](adaptive-threshold-bounds-only-benchmarks.jsonl).

The previous scanner has a discrete 600 s discovery contract. The measured discovery engine has certified/incomplete semantics. **A lower runtime for an incomplete answer is not a speedup for an equivalent observing-window result.** `ratio` is previous/prototype wall time only. Values below 1 mean discovery is slower.

Both paths use the same release binary, evaluator, input, reused context and two Rayon threads. Site preparation and final UTC conversion/output are excluded. Each has one warmup and three timed repetitions (synthetic cases batch 100 calls per repetition). Medians below are milliseconds. Three repetitions do not support a meaningful p95 estimate.

| Scenario | Previous median ms | Discovery median ms | Runtime ratio | Exact calls old/new | Bounds / fits / subdivisions | Unresolved regions | Outcome |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| synthetic/far | 0.0026 | 0.0003 | 8.380 | 145/0 | 1/0/0 | 0 | complete; equivalent classification |
| synthetic/close-pair | 0.0024 | 0.0083 | 0.293 | 145/0 | 59/0/29 | 2 | incomplete; coverage differs |
| synthetic/tangent | 0.0024 | 0.0070 | 0.339 | 145/0 | 41/0/20 | 1 | incomplete; coverage differs |

Synthetic close-pair and tangent cases are more expensive than the cheap point scan when their measured ratio is below one; they establish coverage with analytic envelopes and retain uncertainty near contacts. The scan can miss the synthetic close pair entirely.

Real dynamic cases deliberately disable fits until piecewise branches are isolated. `ALL`, Zodiacal and Airglow certify no time; Moon-only certifies only prepared Moon-down interiors. Starlight uses a test-only experimental map and certifies its cached scalar. These results do not support replacing the production scanner.

Additional per-query counters/times, unresolved duration, and site preparation cost are retained in JSONL. Bounds/fit/refinement timers are summed worker elapsed times, not CPU clocks or total wall time. Batch counters aggregate targets/windows; unresolved days are summed target-time. Counters and stage durations come from the final repetition. Peak pending intervals track work-queue size, not all allocations. Whole-process RSS should be collected separately with `/usr/bin/time -v`; no per-algorithm memory-improvement claim is made.

Reproduce:

```bash
NSB_ADAPTIVE_FILTER=synthetic NSB_ADAPTIVE_SYNTHETIC_FIT_DEPTH=0 RAYON_NUM_THREADS=2 /usr/bin/time -v scripts/benchmark-adaptive-threshold.sh > /tmp/adaptive-benchmark.log
python3 scripts/summarize-adaptive-threshold-benchmark.py /tmp/adaptive-benchmark.log adaptive-threshold-bounds-only-benchmarks
```
