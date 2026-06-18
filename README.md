# nsb — Night Sky Background

Rust workspace for ground-based night-sky background (NSB) modelling and tools.

The runtime library crate is `crates/nsb`. The operational command-line
interface lives in `crates/nsb-cli`. Offline data-generation tools live in
`crates/nsb-data-tools`.

If you are new to astronomy or to the NSB domain, start with:

- `docs/CONCEPTS_AND_IMPLEMENTATION_GUIDE.md` — plain-language explanation of
  the astronomy terms, the query model, and what each implemented component
  means.

Components:

- **Zodiacal light** — Leinert (1998) brightness map, Noll (2012) reddening &
  extinction, scaled solar spectrum.
- **Integrated starlight** — directional Galactic-coordinate map model; disabled
  by default until a real catalogue-derived bundled product is generated with
  provenance.
- **Airglow continuum** — site-bound empirical continuum model with Van Rhijn
  geometry and solar/activity/time corrections.
- **Scattered moonlight** — Jones et al. (2013) wavelength-resolved spectral
  model (default) or Krisciunas & Schaefer (1991) analytic model.

## Architecture

`siderust` owns astronomy, time, coordinates, events, atmosphere, lunar
photometry, passbands, and observatory catalogues. NSB owns NSB-specific
component composition, calibration assets, and planner windows.

Shared reference inputs live in internal `reference` modules; component-specific
calibrations and grids live inside their component modules.

Dependency direction:

```text
nsb-cli        -> nsb
nsb-data-tools -> nsb, when needed
nsb            -> never depends on nsb-cli or nsb-data-tools
```

## Library

The public Rust API is built around a reusable `NsbEvaluator`. Queries take a
`Geodetic<ECEF>` observer directly; named-site parsing is deliberately outside
the library.

`NsbEvaluator::new()` uses the generic clear-sky preset. Starlight is disabled in
that preset because no production Galactic starlight map is bundled yet. Requests
that include `ComponentMask::STARLIGHT` therefore require an explicit custom
`StarlightMap` or the future bundled production map; without one, they fail
explicitly instead of silently using missing or proxy data.

`ComponentMask::ALL` is production-safe and currently means zodiacal light,
airglow, and scattered moonlight. It intentionally excludes starlight until a
production catalogue-derived Galactic starlight map is bundled. Use
`ComponentMask::ALL_SUPPORTED` only with a configuration that supplies an explicit
starlight model.

```rust
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, ThresholdQuery, DEG};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::catalogs::observatories;
use tempoch::Period;

let evaluator = NsbEvaluator::new()?;
let observer = observatories::EL_PARANAL.geodetic();

let r = evaluator.evaluate(&PointQuery {
    observer,
    time: /* tempoch::Time<UTC> */,
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    components: ComponentMask::ALL,
})?;

let w = evaluator.periods_below_threshold(&ThresholdQuery {
    observer,
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    window: Period::new(/* start */, /* end */),
    threshold: BandPhotonRadiance::new(1.0e3),
    components: ComponentMask::ALL,
    sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
    sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
    target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
})?;
```

Runnable Rust examples live under `crates/nsb/examples/`:

- `cargo run -p nsb --example point_query`
- `cargo run -p nsb --example threshold_window`

## CLI

The CLI is implemented by `crates/nsb-cli`:

```bash
cargo run -p nsb-cli -- point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components all

cargo run -p nsb-cli -- window \
  --start 2026-06-18T20:00:00Z \
  --end 2026-06-19T06:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --min-nsb 0.02 \
  --max-nsb 0.25 \
  --components all \
  --format csv
```

Named-site aliases and user-facing parsing live only in the CLI crate, not in the
runtime library.

## Data tools

`crates/nsb-data-tools` is reserved for offline generation and validation tools,
including the planned `build_starlight_map` pipeline. These tools are not runtime
dependencies of `nsb`.

## Validation

End-to-end validation is documented in `docs/VALIDATION.md`. The lightweight CI
suite lives in `crates/nsb/tests/end_to_end_validation.rs` and checks production
`ComponentMask::ALL` point cases, component-sum conservation, explicit starlight
fixture behaviour, threshold-window classification against sampled point curves,
and unrestrictive threshold windows against independent observability intervals.

```bash
cargo test -p nsb --test end_to_end_validation
```

## Build & test

```bash
cargo build --workspace
cargo test --workspace
cargo bench -p nsb
```

## Layout

```text
crates/
├── nsb/             # Runtime library crate, data assets, examples, tests, benches
├── nsb-cli/         # Operational CLI crate
└── nsb-data-tools/  # Offline data-generation tools
docs/                # Supporting notes and historical reports
```

## Documentation

- `docs/CONCEPTS_AND_IMPLEMENTATION_GUIDE.md` — beginner-oriented explanation
  of the domain concepts and the current implementation.
- `docs/VALIDATION.md` — end-to-end validation contract, CI gates, and external
  reference-data process.
- `docs/README.md` — documentation index and pointers to historical reports.
