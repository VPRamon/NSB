# NSB

NSB is a typed Rust library and command-line application for modelling the
ground-based night-sky background and finding observing periods that satisfy an
NSB threshold.

For a specified observer, UTC time, and target direction, NSB composes selected
contributions from zodiacal light, integrated starlight, airglow, and scattered
moonlight. The primary result is integrated photon radiance over 300–650 nm in
`ph cm^-2 ns^-1 sr^-1`, together with per-component scientific metadata and
uncertainty where available.

## Repository modules

| Crate | Purpose |
| --- | --- |
| [`nsb`](crates/nsb) | Typed scientific runtime library, component models, point evaluation, threshold-window search, runtime assets, and maturity metadata |
| [`nsb-cli`](crates/nsb-cli) | Operational CLI, named site aliases, parsing, stable table/JSON/CSV output, and logging |
| [`nsb-data-tools`](crates/nsb-data-tools) | Offline acquisition, transformation, validation, reconciliation, and packaging of scientific data products |
| [`nsb-coverage-gate`](crates/nsb-coverage-gate) | Local overall and diff coverage checks used by CI |

Runtime evaluation never downloads catalogues or invokes data-generation tools.
Scientific assets are prepared offline, validated, checksum-pinned, registered in
the runtime manifest, and embedded or loaded through explicit admission
contracts.

## Scientific interpretation

NSB is production-oriented software, but scientific maturity is
component-specific. A successful build or test suite does not imply that every
component is site-calibrated.

| Component | Default implementation | Current role |
| --- | --- | --- |
| Zodiacal light | Leinert brightness grid, solar spectrum, and Noll-style extinction | Generic clear-sky model |
| Airglow | Empirical continuum with seasonal, nightly, solar, selectable emitting-volume geometry (Van Rhijn default), and independent Noll attenuation terms | Generic model or explicit planning preset |
| Moonlight | Jones et al. (2013) spectral model | Generic model or explicit planning preset |
| KS91 moonlight | Published analytic V-band implementation | Reference/alternate model |
| Integrated starlight | Validated bundled or external HEALPix map; explicit experimental seed/map | Production only after complete asset admission; otherwise experimental |
| CTAO-N / CTAO-S profiles | Explicit atmospheric planning assumptions | Planning presets, not calibrated site products |

B/V magnitude and S10 fields are diagnostic central-wavelength proxies, not
validated Johnson B/V passband integrations. Preserve the returned model,
component, maturity, provenance, uncertainty, and asset metadata in downstream
systems.

See [Model maturity](docs/specifications/model-maturity.md),
[Scientific metadata](docs/specifications/scientific-metadata.md), and the
[Validation matrix](docs/specifications/validation.md) before using results scientifically.

## Quickstart: CLI

Evaluate one target at one instant:

```bash
cargo run --locked -p nsb-cli -- \
  --format json \
  point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components all
```

Find periods below an NSB threshold:

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
  --target-altitude-min 20
```

Global options such as `--format`, `--log-level`, and `-v` precede the
subcommand. Use `nsb sites list` to inspect named observatory aliases, or provide
`--lon`, `--lat`, and `--height` explicitly.

Read the complete [Getting started guide](docs/user-guide/getting-started.md).

## Quickstart: Rust library

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

println!("total: {}", result.integrated);
for component in result.components {
    println!("{}: {}", component.name, component.integrated);
}
# Ok(())
# }
```

Construct and reuse an evaluator. Immutable calibration data and runtime assets
are prepared once rather than reparsed for every query.

## Component and starlight selection

`ComponentMask::ALL`, `ComponentMask::DEFAULT`, and CLI `--components all` are
the same production-safe composition.

- Without an embedded validated production starlight asset, `all` contains
  zodiacal light, airglow, and moonlight.
- With an embedded validated production starlight asset, `all` also contains
  starlight.
- A missing or invalid production starlight product is an error; there is no
  bundled experimental fallback.
- Caller-supplied experimental maps use `StarlightModel::with_experimental_map`
  and are never promoted by that path alone.

A validated external starlight override requires both files:

```bash
nsb --format json point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components starlight \
  --starlight-map /data/starlight.csv \
  --starlight-manifest /data/starlight.toml
```

See [Runtime components](docs/user-guide/components.md) and the
[External starlight manifest](docs/nsb_components/starlight/external-manifest.md).

## Observatory configuration

NSB separates observer coordinates from atmospheric and airglow assumptions.
You can:

- provide arbitrary observatory coordinates;
- select built-in planning profiles;
- adjust supported runtime model inputs;
- provide a validated external starlight product;
- add a new operational site alias;
- develop and validate a new calibrated observatory profile.

The complete workflow is documented in
[Observatory configuration and customisation](docs/user-guide/observatory-customization.md).

## Scientific data generation and updates

`nsb-data-tools` contains the offline commands used to acquire Gaia inputs,
prepare canonical catalogues, build and validate starlight candidates, package
runtime assets, and verify the asset registry.

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- assets verify \
  --manifest crates/nsb/data/manifest.toml
```

Use the [Data-product workflow](docs/maintainer-guide/data-products.md) and the
[Complete data-tool reference](docs/maintainer-guide/tools.md). The normative
command inventory is `crates/nsb-data-tools/tool-registry.toml`.

## Documentation

- [Documentation hub](docs/README.md)
- [User guide](docs/user-guide/README.md)
- [Developer guide](docs/developer-guide/README.md)
- [Maintainer guide](docs/maintainer-guide/README.md)
- [Architecture and modules](docs/developer-guide/architecture.md)
- [Release checklist](docs/operations/release-checklist.md)
- [Changelog](CHANGELOG.md)

## Quality gates

The minimum supported Rust version is 1.89. Pull requests run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
cargo deny check
```

Full Criterion benchmarks are separate from the bounded all-targets smoke path.
See the [Performance contract](docs/specifications/performance.md).

## Licensing

NSB source is licensed under AGPL-3.0-only; see [`LICENSE`](LICENSE).
Third-party dependencies and scientific assets retain their own licence and
attribution requirements. Distributors must review and comply with those terms;
unknown upstream asset terms are treated as release limitations, not guessed.
