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
  provenance and quantitative validation.
- **Airglow continuum** — site-bound empirical continuum model with Van Rhijn
  geometry and solar/activity/time corrections.
- **Scattered moonlight** — Jones et al. (2013) wavelength-resolved spectral
  model (default) or Krisciunas & Schaefer (1991) analytic model.

## Architecture

`siderust` owns astronomy, time, coordinates, events, atmosphere, lunar
photometry, passbands, and observatory catalogues. NSB owns NSB-specific
component composition, calibration assets, planner windows, and site-profile
metadata.

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

`NsbEvaluator::new()` uses `NsbModelConfig::generic_clear_sky()`. This is an
explicit generic clear-sky development/planning baseline, not a production-grade
or CTAO-validated science preset. Starlight is disabled in that preset because
no catalogue-derived Galactic starlight map is bundled and quantitatively
validated yet. Requests that include `ComponentMask::STARLIGHT` therefore
require an explicit custom `StarlightMap` or the future bundled catalogue map;
without one, they fail explicitly instead of silently using missing or proxy
data.

Preset names intentionally encode maturity:

- `generic_clear_sky()` is the current default baseline.
- `NsbModelConfig::cta_n_planning()` and `NsbModelConfig::cta_s_planning()`
  select explicit CTAO planning profiles with machine-readable atmospheric,
  airglow, and provenance assumptions. They are not marked as fully
  site-calibrated until dedicated CTAO validation data are bundled.
- `python_parity()` is hidden and reserved for historical regression parity.
- Names such as `standard` or `best_science` are not exposed until a complete,
  reproducible, and quantitatively validated model configuration exists.

Use named profiles at CTAO call sites instead of relying on generic fallback
constructors:

```rust
use nsb::{Airglow, Jones2013Spectral, NsbEvaluator, NsbModelConfig, SiteProfileId};

let evaluator = NsbEvaluator::with_config(NsbModelConfig::cta_s_planning())?;
let moonlight = Jones2013Spectral::for_site_profile(observer, SiteProfileId::CtaSouth);
let airglow = Airglow::for_site_profile(observer, SiteProfileId::CtaSouth)?;
let profile = SiteProfileId::CtaSouth.profile(observer);
assert!(!profile.is_site_calibrated());
```

Detailed CTAO profile assumptions live in `docs/CTAO_SITE_PROFILES.md`.

`ComponentMask::ALL` currently means zodiacal light, airglow, and scattered
moonlight. It intentionally excludes starlight until a catalogue-derived Galactic
starlight map is bundled and validated. Use `ComponentMask::ALL_SUPPORTED` only
with a configuration that supplies an explicit starlight model.

Every `NsbComponent` carries `NsbComponentMetadata`: calibration status,
provenance, validated-domain notes, and the B/V diagnostic convention. The
current B/V S10 fields and `b_mag`/`v_mag` totals are explicitly
monochromatic central-wavelength diagnostics at 445 nm and 551 nm, not Johnson
B/V passband integrations. Airglow also exposes a relative one-sigma uncertainty
when the empirical continuum calibration provides one. See
`docs/SCIENTIFIC_METADATA.md` for the error-budget and provenance contract.

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
suite lives in `crates/nsb/tests/end_to_end_validation.rs` and checks generic
clear-sky `ComponentMask::ALL` point cases, component-sum conservation, explicit
starlight fixture behaviour, threshold-window classification against sampled
point curves, and unrestrictive threshold windows against independent
observability intervals. Component-level validation also covers Jones 2013
reference-fixture structure, zodiacal Leinert anchor values, Noll-style
extinction numerics, and the public science metadata contract.

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
- `docs/SCIENTIFIC_METADATA.md` — calibration status, provenance, uncertainty,
  B/V proxy convention, and first-order component error budget.
- `docs/CTAO_SITE_PROFILES.md` — machine-readable site-profile assumptions and
  calibration maturity for CTAO planning presets.
- `docs/VALIDATION.md` — end-to-end validation contract, CI gates, and external
  reference-data process.
- `docs/README.md` — documentation index and pointers to historical reports.
