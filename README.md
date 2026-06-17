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
- **Integrated starlight** — directional Galactic-coordinate map model; the
  standard map is not bundled until a real catalogue-derived product is
  generated with provenance.
- **Airglow continuum** — SkyCalc-continuum path with Van Rhijn geometry
  (default) or empirical cubic polynomial in source altitude.
- **Scattered moonlight** — Jones et al. (2013) wavelength-resolved spectral
  model (default) or Krisciunas & Schaefer (1991) analytic model.

## Architecture

`siderust` owns astronomy, time, coordinates, events, atmosphere, lunar
photometry, and passbands. NSB owns only NSB-specific tables, component
composition, and planner windows:

- Atmosphere (Rayleigh optical depth, Mie optical depth, airmass, Van Rhijn,
  phase functions) — `siderust::atmosphere`.
- Solar night and target altitude windows — `siderust::event::altitude`.
- Horizontal geometry — `siderust::event::horizontal`.
- Lunar photometry — `siderust::event::lunar::photometry`.
- Observatory geodetic coordinates — `siderust::catalogs::observatories`.

Bundled scientific tables and their references:

| Table / asset | Role | Official reference or authoritative upstream | Notes |
|---|---|---|---|
| `src/data/leinert.rs` | Zodiacal-light brightness grid | Leinert, Ch., et al. (1998), *A&AS* **127**, 1, "The 1997 reference of diffuse night sky brightness" | Core empirical zodiacal table used by `components::zodiacal` |
| `tools/build_starlight_map/` | Standard starlight-map generator placeholder | No production catalogue product is bundled yet | `Starlight::standard_galactic_model()` returns `DataMissing` until `data/starlight_galactic_map_v1.csv` exists |
| `data/airglow_cont.dat` | Wavelength-resolved airglow continuum table | ESO SkyCalc / Advanced Cerro Paranal Sky Model lineage, as described by Noll, S., et al. (2012), *A&A* **543**, A92 | Multi-block continuum table with season/time corrections |
| `data/solar_spectrum.dat` | Solar reference spectrum used to shape zodiacal light | Bundled `darknsb`/SkyCalc lineage artifact | Direct publication/source file provenance is not yet recorded separately in-repo |
| `data/mie_m15s1.dat` | Aerosol/Mie scattering phase grid | Bundled `darknsb`/SkyCalc lineage artifact | Direct generator metadata is still being documented |
| `data/sscatcor_m15s1.dat` | Multiple-scattering correction grid | Bundled `darknsb`/SkyCalc lineage artifact | Direct generator metadata is still being documented |
| `data/lut_moon/*.csv` | Precomputed moonlight lookup tables | Bundled `darknsb`/CTAO operational artifact | Generated LUT family, not a primary published table |
| `siderust::atmosphere::ozone::transmission_table()` | Ozone transmittance table reused from `siderust` | Patat, F., et al. (2008), *A&A* **481**, 575, "An Atlas of the Sky Background Spectrum over Cerro Paranal" | Canonical copy lives upstream in `siderust` |

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
    components: ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
})?;

// 2) UTC sub-periods darker than a threshold within a window:
let w = evaluator.periods_below_threshold(&ThresholdQuery {
    location: Location::NamedSite(Site::Paranal),
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    window: Period::new(/* start */, /* end */),
    threshold: BandPhotonRadiance::new(1.0e3),
    components: ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
    sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
    sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
    target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
})?;
```

The threshold search uses an event-driven pipeline modelled on
`siderust::event::stellar::altitude_periods`: it pre-filters the
window to the intersection of *Sun below astronomical twilight* and
*target above the horizon*, then runs a coarse scan with Brent
refinement (via `siderust::numeric::intervals`) inside each
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
  --component zodiacal --component airglow
```

### Threshold-window search

```bash
cargo run --bin nsb -- window \
  --start 2023-09-04T00:00:00Z --end 2023-09-04T12:00:00Z \
  --threshold 5e2 \
  --site CTAO-S \
  --ra 266.41683 --dec -29.00781 \
  --component zodiacal --component airglow
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
or `--all`. The default is `zodiacal + airglow`. `starlight` currently
requires a generated `data/starlight_galactic_map_v1.csv`; requesting it before
that file exists returns `DataMissing`.

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
examples/              # Runnable library examples
docs/                  # Supporting notes and historical reports
src/
├── evaluator.rs       # NsbEvaluator + PointQuery + ThresholdQuery + Location
├── site.rs            # Named CTAO sites (Paranal / La Palma)
├── sites.rs           # Broader observatory catalogue with k_v metadata
├── leinert.rs         # Leinert (1998) S10 zodiacal table
├── single_scatter.rs  # Mie phase + multiple-scattering correction parsers
├── error.rs
├── components/        # ZL, SL, AG, Moon component evaluators
├── spectra/           # Solar / airglow / ozone loaders
└── bin/nsb.rs         # CLI binary
```

## Sibling crates

This crate depends, via `path = ".."`, on:

- [`qtty`](../qtty) — quantity / unit types
- [`tempoch`](../tempoch) — astronomical time scales
- [`siderust`](../siderust) — coordinates, observatories, ephemerides, atmosphere
- [`optica`](../optica) — spectra and grid interpolation

## Documentation

- `docs/CONCEPTS_AND_IMPLEMENTATION_GUIDE.md` — beginner-oriented explanation
  of the domain concepts and the current implementation.
- `docs/README.md` — documentation index and pointers to historical reports.
