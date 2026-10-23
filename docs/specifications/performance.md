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
a partial schedule.

This is an empirical model-resolution contract, not continuous-time proof.
Strict certified methods use the same engine but accept only sound interval
enclosures; active dynamic models remain unsupported by that stronger contract.
See the [adaptive search contract](adaptive-search.md) and
[scientific validation](adaptive-validation.md). The 2026-10-13
three-way timing study and 204-scenario engineering audit are
[archived at a fixed commit](https://github.com/VPRamon/NSB/blob/421cf9db5ab08ffe158120193fa8af40f2ea642b/docs/specifications/adaptive-performance.md);
they describe their historical binaries and dependency graph, not the
performance of every future release.

## Performance-neutral implementation rules

- Bundled and external production Starlight maps use the same canonical
  admission checks: sidecar validation, checksum/header verification, HEALPix
  geometry validation, finite value/uncertainty validation, flux conservation,
  and map diagnostics.
- Airglow threshold evaluation may use allocation-free integrated scalar paths
  only when equivalence tests cover the supported domain.
- Resolved F10.7 values may be cached by stable UTC date; issue/retrieval
  transition dates bypass unsafe reuse.
- A `SiteWindowContext` may be reused across targets and thresholds when its
  invariant fields match. Exact ephemeris reuse must preserve the original
  instant and model state.
- Astronomical event finding remains in Siderust. Adaptive fits require
  authoritative validation and cannot substitute for interval certificates.

## General benchmark commands

```bash
cargo bench -p nsb --bench threshold_window
cargo bench -p nsb --bench airglow_geometry
```

Normal CI compiles and smoke-runs benchmark binaries through
`cargo test --workspace --all-targets --locked`; the full adaptive acceptance
matrix is an explicit release run. Do not use historical scanner/adaptive timing results as current
performance assertions or automated pass/fail budgets. Record fresh
measurements for performance-sensitive changes, with the toolchain,
workload, dependency graph and hardware stated explicitly.
