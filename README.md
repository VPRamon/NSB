# NSB

NSB is a typed Rust library and CLI for modelling ground-based night-sky
background and finding observing windows. The repository separates runtime
evaluation (`nsb`), operational presentation (`nsb-cli`), and offline scientific
data preparation (`nsb-data-tools`).

The software and release gates are production-oriented. Scientific maturity is
component-specific and is never inferred from software quality: the default is a
generic/planning model, not a site-calibrated CTAO product.

## Model contract

`ComponentMask::ALL`, `ComponentMask::DEFAULT`, and CLI `--components all` are
identical:

- zodiacal light;
- airglow continuum;
- scattered moonlight.

Integrated starlight is outside that set. The repository contains a
low-resolution manual seed only for pipeline and lookup tests. Library users
must opt into `StarlightModel::bundled_experimental_seed()` for that seed.
Production starlight currently uses `StarlightModel::validated_external(...)`,
which can only be constructed from a checksum-pinned map and complete validation
sidecar. A maintainer-only Gaia DR3 XP release pipeline can derive a bundled map
candidate from local Gaia extracts, but raw Gaia rows are not embedded and the
bundled Gaia asset remains pending until release validation and independent
comparison evidence are complete. CLI `starlight` requires both
`--starlight-map` and `--starlight-manifest`; `experimental-starlight` names
only the seed. There is no fallback between them.

| Component | Default implementation | Maturity |
|---|---|---|
| Zodiacal light | Leinert (1998), solar spectrum, Noll-style extinction | Generic clear sky |
| Airglow | Empirical continuum with seasonal, nightly, solar, and Van Rhijn terms | Generic or planning preset |
| Moonlight | Jones et al. (2013) spectral model | Generic or planning preset |
| KS91 moonlight | Published analytic V-band alternate | Published reference |
| Integrated starlight | Validated external map, future bundled Gaia-derived map, or bundled manual seed | Production only when sidecar admission passes or Gaia asset validation is complete; otherwise experimental; non-default |
| CTAO-N / CTAO-S profiles | Explicit atmospheric planning assumptions | Planning preset, not calibrated |

The integrated output is photon radiance over 300–650 nm. B/V S10 and magnitude
fields are diagnostic central-wavelength proxies, not validated Johnson B/V
passband integrations. JSON and CSV preserve this distinction.

See [model maturity](docs/MODEL_MATURITY.md) and
[validation](docs/VALIDATION.md) before using results scientifically.

## Rust quickstart

```bash
cargo run -p nsb --example point_query
```

Expected output begins with the component breakdown and ends approximately as:

```text
    total: 1.947374e-1 ph/(cm² ns sr)
       B = 22.340 mag/arcsec²
       V = 22.015 mag/arcsec²
```

Library construction is reusable; immutable calibration data are parsed once:

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
assert_eq!(result.components.len(), 3);
# Ok(())
# }
```

## CLI quickstart

Global output options precede the subcommand:

```bash
cargo run -p nsb-cli -- --format json point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components all
```

Expected JSON contains these audit fields in addition to numeric results:

```json
{
  "schema_version": "nsb-cli-point-json-v1",
  "version": {
    "model_version": "nsb-model-2026.1",
    "siderust_revision": "8d94b8375ae23c26d00346f74951e52cd1b595cc",
    "asset_manifest_schema": 1
  },
  "model": { "preset": "ctao-south-planning" },
  "components": [
    { "name": "zodiacal", "metadata": { "calibration_status": "generic-clear-sky" } }
  ]
}
```

The stable CSV schemas are documented in [CLI schemas](docs/CLI_SCHEMAS.md).

## Reproducibility and assets

Siderust 0.10.1 is resolved from the local workspace checkout at
`../../siderust`; the current compatibility baseline is revision
`8d94b8375ae23c26d00346f74951e52cd1b595cc`. All CI builds use `Cargo.lock`.
Compatibility and update policy are in
[SIDERUST_COMPATIBILITY.md](docs/SIDERUST_COMPATIBILITY.md).

Every file under `crates/nsb/data` is registered in
`crates/nsb/data/manifest.toml`. Verify files, checksums, required provenance,
and starlight header consistency with:

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

The manifest honestly records provenance gaps in inherited files. Such gaps
prevent a component from being promoted to calibrated production science.
Externally supplied production starlight is not a bundled asset and therefore
uses its own strict sidecar contract documented in
[external starlight manifests](docs/EXTERNAL_STARLIGHT_MANIFEST.md).

## Quality gates

MSRV is Rust 1.89, matching pinned Siderust's SIMD dependency floor. Pull
requests run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
cargo deny check
```

Benchmarks are scheduled/manual and cover point components, experimental
starlight lookup, full composition, and window searches.

## Documentation

- [Production roadmap](docs/PRODUCTION_ROADMAP.md)
- [Performance contract](docs/PERFORMANCE.md)
- [Model maturity](docs/MODEL_MATURITY.md)
- [Scientific metadata](docs/SCIENTIFIC_METADATA.md)
- [Validation matrix](docs/VALIDATION.md)
- [Starlight pipeline](docs/STELLAR_MAP_GENERATION.md)
- [CTAO planning profiles](docs/CTAO_SITE_PROFILES.md)
- [Release checklist](docs/RELEASE_CHECKLIST.md)
- [Changelog](CHANGELOG.md)

NSB source uses BSD-3-Clause. The dependency graph intentionally includes
AGPL-3.0-only astronomy crates, including Siderust; distributors of combined
binaries must comply with those dependency terms. Individual scientific assets
have separate manifest records. Unknown upstream terms are treated as release
limitations, not guessed.
