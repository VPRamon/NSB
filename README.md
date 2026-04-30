# nsb — Night Sky Background

Rust crate and CLI for the ground-based night-sky background (NSB) photon
flux in `ph/(cm² · ns · sr)`, plus the integrated B and V band surface
brightness in `mag/arcsec²`.

If you are new to astronomy or to the NSB domain, start with:

- `docs/CONCEPTS_AND_IMPLEMENTATION_GUIDE.md` — plain-language explanation of
  the astronomy terms, the query model, and what each implemented component
  means.

Components:

- **Zodiacal light** — Leinert (1998) brightness map, Noll (2012) reddening &
  extinction, scaled solar spectrum.
- **Integrated starlight** — SkyCalc Cerro Paranal radiance.
- **Airglow continuum** — empirical cubic in source altitude (Noll 2012).
- **Scattered moonlight** — Krisciunas & Schaefer (1991) analytic
  scattered-moonlight model, converted into the crate's integrated radiance
  output.

## Library

The public Rust API is built around a single `NsbEvaluator` and two query
shapes:

```rust
use nsb::{ComponentMask, Location, NsbEvaluator, PointQuery, Site, Target, ThresholdQuery, DEG};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use tempoch::{Period, Time, UTC};

let evaluator = NsbEvaluator::new()?;

// 1) NSB at one (time, location, target):
let r = evaluator.evaluate(&PointQuery {
    location: Location::NamedSite(Site::Paranal),
    time: /* tempoch::Time<UTC> */,
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    components: ComponentMask::ZODIACAL | ComponentMask::STARLIGHT | ComponentMask::AIRGLOW,
})?;

// 2) UTC sub-periods darker than a threshold within a window:
let w = evaluator.periods_below_threshold(&ThresholdQuery {
    location: Location::NamedSite(Site::Paranal),
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    window: Period::new(/* start */, /* end */),
    threshold: BandPhotonRadiance::new(1.0e3),
    components: ComponentMask::ALL,
    sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
    sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
    target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
})?;
```

The threshold search uses an event-driven pipeline modelled on
`siderust::calculus::stellar::altitude_periods`: it pre-filters the
window to the intersection of *Sun below astronomical twilight* and
*target above the horizon*, then runs a coarse scan with Brent
refinement (via `siderust::calculus::math_core::intervals`) inside each
candidate sub-window. For year-long searches this typically delivers
a 1–2 order of magnitude speedup over a uniform-cadence scan. Setting
`sun_altitude_ceiling` and/or `target_altitude_floor` to `None`
disables the corresponding pre-filter (legacy uniform-scan semantics).

Runnable Rust examples live under `examples/`:

- `cargo run --example point_query`
- `cargo run --example threshold_window`

## CLI

The crate ships an `nsb` binary with two subcommands.

### Point evaluation

```bash
cargo run --bin nsb -- point \
  --time 2023-09-04T01:48:00Z \
  --site CTAO-S \
  --ra 266.41683 --dec -29.00781
```

Or with arbitrary geodetic coordinates:

```bash
cargo run --bin nsb -- point \
  --time '2023-09-04 01:48:00' \
  --lat -24.6275 --lon -70.4044 --alt 2635 \
  --ra 266.41683 --dec -29.00781 \
  --component zodiacal --component starlight --component airglow
```

### Threshold-window search

```bash
cargo run --bin nsb -- window \
  --start 2023-09-04T00:00:00Z --end 2023-09-04T12:00:00Z \
  --threshold 5e2 \
  --site CTAO-S \
  --ra 266.41683 --dec -29.00781 \
  --all
```

Pre-filter knobs (defaults reproduce the recommended pipeline):

* `--sun-altitude-ceiling -18` — drop sub-windows where the Sun is above
  astronomical twilight. Pass `90` to disable.
* `--target-altitude-floor 0` — drop sub-windows where the target is
  below the horizon. Pass `-90` to disable.
* `--no-pre-filter` — disable both pre-filters; equivalent to the legacy
  uniform-scan semantics.
* `--step-seconds 600` — coarse-scan cadence (Brent refines crossings to
  seconds inside each candidate sub-window).

Component selection: `--component zodiacal|starlight|airglow|moon` (repeatable)
or `--all`. The default is `zodiacal + starlight + airglow`.

## Examples

The repository includes two small end-to-end examples for the current API:

- `examples/point_query.rs` — evaluate the NSB for one UTC instant, location,
  and equatorial target.
- `examples/threshold_window.rs` — search a UTC time window for periods darker
  than a radiance threshold.

## Build & test

```bash
cargo build
cargo test
cargo bench   # Criterion benches under benches/threshold_window.rs
```

## Layout

```
examples/          # Runnable library examples
docs/              # Supporting notes and historical reports
src/
├── evaluator.rs   # NsbEvaluator + PointQuery + ThresholdQuery + Location
├── site.rs        # Named CTAO sites
├── error.rs
├── components/    # ZL, SL, AG, Moon
├── spectra/       # Solar / starlight / airglow / ozone loaders + B/V filters
├── atmosphere/    # Rayleigh / Mie / single-scatter
├── data/          # Embedded Leinert table + bundled .dat files
└── bin/nsb.rs     # CLI binary
```

## Sibling crates

This crate depends, via `path = ".."`, on:

- [`qtty`](../qtty) — quantity / unit types
- [`tempoch`](../tempoch) — astronomical time scales
- [`siderust`](../siderust) — coordinates, observatories, ephemerides
- [`affn`](../affn) — affine geometry primitives
- [`cheby`](../cheby) — Chebyshev interpolation

## Documentation

- `docs/CONCEPTS_AND_IMPLEMENTATION_GUIDE.md` — beginner-oriented explanation
  of the domain concepts and the current implementation.
- `docs/README.md` — documentation index and pointers to historical reports.
