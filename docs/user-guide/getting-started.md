# Getting started

Status: Current quickstart for the workspace.
Audience: CLI users and Rust library users.
Scope: Build, point evaluation, window search, site selection, and first API use.

## Prerequisites

NSB is a Cargo workspace. The current minimum supported Rust version is 1.89.
Run commands from the repository root and use the lockfile for reproducible
builds.

```bash
cargo build --workspace --locked
cargo test --workspace --locked
```

The workspace contains three crates:

- `nsb`: typed runtime library;
- `nsb-cli`: operational command-line interface, installed as `nsb`;
- `nsb-data-tools`: offline scientific data-product tooling for maintainers.

## Point evaluation

Evaluate the sky background at one UTC instant:

```bash
cargo run --locked -p nsb-cli -- \
  --format table \
  point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components all
```

Global options such as `--format`, `--log-level`, and `-v` must appear before the
subcommand. Available output formats are `table`, `json`, and `csv`.

Use explicit geodetic coordinates instead of a named site when required:

```bash
cargo run --locked -p nsb-cli -- \
  --format json \
  point \
  --time 2026-06-18T23:00:00Z \
  --lon -70.406944 \
  --lat -24.627222 \
  --height 2100 \
  --ra 83.6331 \
  --dec 22.0145
```

Longitude is east-positive, latitude is north-positive, and height is the
ellipsoidal height in metres.

## Window search

Find periods below a maximum NSB radiance:

```bash
cargo run --locked -p nsb-cli -- \
  --format csv \
  window \
  --start 2026-06-18T20:00:00Z \
  --end 2026-06-19T06:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --max-nsb 0.25 \
  --sun-altitude-max -18 \
  --target-altitude-min 20 \
  --step 600
```

`--step` controls the coarse scan interval. Threshold crossings are refined by
the search implementation; it is not simply a list of accepted coarse samples.
Use `--no-pre-filter` only when the Sun and target-altitude filters must be
disabled explicitly.

## Inspect supported sites

```bash
cargo run --locked -p nsb-cli -- sites list
cargo run --locked -p nsb-cli -- sites show CTAO-S
```

Named aliases provide coordinates. CTAO-N and CTAO-S additionally select their
corresponding planning profiles. Other aliases currently use the generic
clear-sky profile unless the application configures the library directly.

## Select components and model options

```text
zodiacal
starlight
experimental-starlight
airglow
moon
all
```

Component names may be comma-separated. Useful model options include:

- `--moonlight-model jones2013` or `ks1991`;
- `--solar-radio-flux-sfu <value>` for the airglow solar-activity input;
- `--zodiacal-extinction noll2012` or `none`;
- `--starlight-map` together with `--starlight-manifest` for a validated external
  production starlight map.

See [Runtime components](components.md) before changing the default composition.

## Configuration templates

The CLI can generate and validate a TOML configuration structure:

```bash
cargo run --locked -p nsb-cli -- config init > nsb.toml
cargo run --locked -p nsb-cli -- config validate nsb.toml
```

The current CLI validates this schema but point and window commands still take
their operational values from command-line arguments. Treat the generated file
as a reproducible configuration template until direct `--config` execution is
implemented.

## Rust library

The library accepts typed observer, time, and target values directly:

```rust,no_run
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;

# fn evaluate(time: tempoch::Time<tempoch::UTC>) -> nsb::Result<()> {
let evaluator = NsbEvaluator::new()?;
let result = evaluator.evaluate(&PointQuery {
    observer: observatories::EL_PARANAL.geodetic(),
    time,
    target: Target::new(266.41683 * DEG, -29.00781 * DEG),
    components: ComponentMask::ALL,
})?;

println!("{}", result.integrated);
for component in &result.components {
    println!("{}: {}", component.name, component.integrated);
}
# Ok(())
# }
```

Construct one `NsbEvaluator` and reuse it. Immutable calibration tables and
assets are prepared at evaluator construction rather than reparsed for every
query.

## Read the output correctly

JSON and CSV are intended for downstream automation. Preserve the following
fields when storing or comparing results:

- schema and model versions;
- selected site profile and calibration status;
- per-component maturity and provenance;
- uncertainty fields;
- asset checksums and manifest schema;
- the diagnostic meaning of B/V fields.

The stable output contract is documented in [CLI schemas](../CLI_SCHEMAS.md).