# Performance contract

Status: Current correctness and performance evidence for long-horizon threshold
searches. Timings are reference measurements, not portable pass/fail thresholds.

`NsbEvaluator` owns immutable parsed component data. Planning owns
`SiteWindowContext`, the target-independent reuse boundary for workloads with
one evaluator, site, UTC window, component mask, and Sun filter. Target,
target-altitude floor, radiance threshold, and sample step may vary. The context
validates its owner and invariant fields before every reuse.

## Correctness contract

Normal Rust/Python threshold methods and CLI `window` use physics-partitioned,
step-free adaptive discovery. Checked Chebyshev fits guide exact integrated NSB
probes, extrema searches and crossing refinement. Crossing brackets are at most
0.1 seconds and export their accepted sides. Inclusive bands share the search
for both thresholds. Numerical ambiguity or exhaustion returns an error without
a partial schedule. Legacy step inputs are retained but ignored.

This is an empirical model-resolution contract, not continuous-time proof.
Strict certified methods use the same engine but accept only sound interval
enclosures; active dynamic models remain unsupported by that stronger contract.
See the [adaptive search contract](adaptive-search.md),
[scientific validation](adaptive-validation.md), and
[production measurements](adaptive-performance.md).
