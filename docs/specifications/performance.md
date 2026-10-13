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
See the [scientific assumptions](adaptive-threshold-production-status.md),
[complete matrix](adaptive-acceptance/production.md) and
[performance decision](adaptive-acceptance/performance-summary.md).
The scanner is an independent test-only reference. Historical timings below
predate the default migration and do not measure the current adaptive engine.

Astronomical event finding belongs to Siderust:

- Sun filters and astronomical-night boundaries use Siderust solar
  altitude-event handling.
- Fixed-target visibility uses Siderust `event::altitude` for the ICRS
  direction.
- Moon visibility uses Siderust `event::altitude` for the Moon at the
  geometric horizon.

NSB owns composition of these physical periods, Airglow phase semantics, exact
component evaluation, adaptive radiance discovery, crossing refinement, result
coalescing, and context reuse.

Production prepares independent Sun/Airglow and Moon state concurrently, then
processes independent threshold windows with Rayon. Collection order and final
coalescing are deterministic.

## Performance-neutral implementation rules

- Bundled and external production Starlight maps use the same canonical
  admission checks: sidecar validation, checksum/header verification, HEALPix
  geometry validation, finite value/uncertainty validation, flux conservation,
  and map diagnostics.
- Airglow threshold evaluation may use allocation-free integrated scalar paths
  only when equivalence tests cover the supported domain.
- Resolved F10.7 values may be cached by stable UTC date; issue/retrieval
  transition dates bypass unsafe reuse.
- A `SiteWindowContext` may be reused across many targets and thresholds when
  its invariant fields match.
- Exact point evaluations may reuse ephemeris state. Adaptive fits must be
  validated with the authoritative evaluator and must never be promoted to
  strict interval certificates.

## Historical scanner measurement

Reference environment recorded for the 0.1.0 baseline:

```text
CPU: AMD Ryzen 7 5700X, 8 cores / 16 threads, 32 MiB L3
OS: Linux 7.0.0-31-generic x86_64
Rust: rustc 1.97.0 (2d8144b78 2026-07-07)
Cargo: cargo 1.97.0 (c980f4866 2026-06-30)
Build: cargo build --release --locked -p nsb-cli --bin nsb
Resource cap: CARGO_BUILD_JOBS=2, RAYON_NUM_THREADS=2, nice -n 10
```

Representative one-year CTAO-S threshold search:

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

Three resource-capped production runs measured 3.27, 3.29, and 3.13 seconds
(median 3.27 seconds), with approximately 58 MiB maximum resident memory.

The corresponding Criterion reference was:

```text
threshold_window_duration_component/all/1y
time: [3.5184 s 3.5306 s 3.5460 s]
```

## Benchmark and verification commands

```bash
cargo bench -p nsb --bench threshold_window
cargo bench -p nsb --bench threshold_window \
  threshold_window_duration_component/all/1y -- \
  --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10 --noplot
cargo bench -p nsb --bench airglow_geometry
```

Normal CI compiles and smoke-runs benchmark binaries through
`cargo test --workspace --all-targets --locked`; it does not run the full
Criterion matrix.

Performance changes must preserve exact component outputs, keep astronomical
event ownership in Siderust, refine reported brackets with authoritative values,
and document model-resolution assumptions and empirical discrepancy checks.
Current production comparisons use the same upgraded dependency graph and
complete usable schedules, including annual 100-target workloads.
