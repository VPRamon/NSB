# nsb — Night Sky Background

Rust library for ground-based night-sky background (NSB) photon flux in
`ph/(cm² · ns · sr)`, plus integrated B and V band surface brightness in
`mag/arcsec²`.

This crate is intentionally library-only. Command-line parsing, named-site
aliases, output formatting, and operational presets should live in a separate
CLI crate that consumes `nsb`.

If you are new to astronomy or to the NSB domain, start with:

- `docs/CONCEPTS_AND_IMPLEMENTATION_GUIDE.md` — plain-language explanation of
  the astronomy terms, the query model, and what each implemented component
  means.

Components:

- **Zodiacal light** — Leinert (1998) brightness map, Noll (2012) reddening &
  extinction, scaled solar spectrum.
- **Integrated starlight** — directional Galactic-coordinate map model; the
  standard map is not bundled until a real catalogue-derived product is
  generated with provenance.
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

## Library

The public Rust API is built around a reusable `NsbEvaluator`. Queries take a
`Geodetic<ECEF>` observer directly; named-site parsing is deliberately outside
the library.

```rust
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, ThresholdQuery, DEG};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::catalogs::observatories;
use tempoch::Period;

let evaluator = NsbEvaluator::new()?;
let observer = observatories::EL_PARANAL.geodetic();

// 1) NSB at one (time, observer, target):
let r = evaluator.evaluate(&PointQuery {
    observer,
    time: /* tempoch::Time<UTC> */,
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    components: ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
})?;

// 2) UTC sub-periods darker than a threshold within a window:
let w = evaluator.periods_below_threshold(&ThresholdQuery {
    observer,
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    window: Period::new(/* start */, /* end */),
    threshold: BandPhotonRadiance::new(1.0e3),
    components: ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
    sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
    sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
    target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
})?;
```

Runnable Rust examples live under `examples/`:

- `cargo run --example point_query`
- `cargo run --example threshold_window`

## Build & test

```bash
cargo build
cargo test
cargo bench   # Criterion benches under benches/threshold_window.rs
```

## Layout

```text
examples/              # Runnable library examples
docs/                  # Supporting notes and historical reports
src/
├── evaluator.rs       # NsbEvaluator + PointQuery + ThresholdQuery
├── error.rs
├── components/        # Zodiacal, starlight, airglow, moonlight models
└── reference/         # Internal shared reference data
```

## Sibling crates

This crate depends on:

- `qtty` — quantity / unit types
- `tempoch` — astronomical time scales
- `siderust` — coordinates, observatories, ephemerides, atmosphere
- `optica` — spectra and grid interpolation

## Documentation

- `docs/CONCEPTS_AND_IMPLEMENTATION_GUIDE.md` — beginner-oriented explanation
  of the domain concepts and the current implementation.
- `docs/README.md` — documentation index and pointers to historical reports.
