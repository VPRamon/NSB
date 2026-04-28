# nsb — Night Sky Background

Rust crate and CLI for the ground-based night-sky background (NSB) photon
flux in `ph/(cm² · ns · sr)`, plus the integrated B and V band surface
brightness in `mag/arcsec²`.

Components:

- **Zodiacal light** — Leinert (1998) brightness map, Noll (2012) reddening &
  extinction, scaled solar spectrum.
- **Integrated starlight** — SkyCalc Cerro Paranal radiance.
- **Airglow continuum** — empirical cubic in source altitude (Noll 2012).
- **Scattered moonlight** — currently a stub returning zero.

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
})?;
```

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

Component selection: `--component zodiacal|starlight|airglow|moon` (repeatable)
or `--all`. The default is `zodiacal + starlight + airglow`.

## Build & test

```bash
cargo build
cargo test
```

## Layout

```
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

- `docs/README.md` — documentation index.
