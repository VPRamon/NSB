# nsb

Night-sky background evaluation and observation planning for ground-based
astronomy.

This is the scientific runtime library for NSB. It provides typed point
evaluation and observing-window search over the production-safe composition of
zodiacal light, airglow, scattered moonlight, and an admitted integrated
starlight product when one is available.

## Installation

```bash
cargo add nsb@0.1.0
```

The minimum supported Rust version is 1.89.

NSB deliberately uses domain types from Siderust and tempoch at public
boundaries instead of duplicating coordinate, atmosphere, or time semantics.
Applications that construct those upstream types directly should add the
corresponding dependencies as needed.

## Minimal Rust workflow

```rust,no_run
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;

# fn evaluate(time: tempoch::Time<tempoch::UTC>) -> nsb::Result<()> {
let evaluator = NsbEvaluator::new()?;
let query = PointQuery::new(
    observatories::EL_PARANAL.geodetic(),
    time,
    Target::new(266.41683 * DEG, -29.00781 * DEG),
)
.with_components(ComponentMask::ALL);

let result = evaluator.evaluate(&query)?;
println!("{}", result.integrated);
for component in &result.components {
    println!("{}: {}", component.name, component.integrated);
}
# Ok(())
# }
```

Construct an evaluator once and reuse it. Immutable model data and runtime
assets are prepared during evaluator construction.

## Scientific interpretation

`ComponentMask::DEFAULT` and `ComponentMask::ALL` are the same
production-safe composition for the `0.1.x` model contract. Starlight enters
that composition only when a validated production map is embedded. The
release candidate carried in the repository is not redistributed in the
published `0.1.0` package while its licensing approval remains pending.

Observatory coordinates and scientific site assumptions are separate. The core
crate exposes generic typed site profiles; CTAO North/South planning presets
belong to application layers and are not site-calibration claims.

B/V magnitude and S10 outputs are central-wavelength diagnostics, not validated
Johnson B/V passband integrations. Consult component maturity, provenance,
validated-domain, uncertainty, and asset metadata before making scientific or
operational claims.

## Interfaces and documentation

- Rust API: <https://docs.rs/nsb>
- Repository documentation: <https://github.com/VPRamon/NSB/tree/main/docs>
- Scientific model: <https://github.com/VPRamon/NSB/blob/main/docs/specifications/scientific-model.md>
- Model maturity: <https://github.com/VPRamon/NSB/blob/main/docs/specifications/model-maturity.md>
- Validation matrix: <https://github.com/VPRamon/NSB/blob/main/docs/specifications/validation.md>
- Python distribution: `nsb-rust` on PyPI, imported as `nsb`
- CLI: sibling workspace crate `nsb-cli` (binary name `nsb`; not separately
  published in `0.1.0`)

The CLI owns command-line parsing, named-site lookup, and stable JSON/CSV
rendering. Offline catalogue acquisition and scientific data-product generation
belong to `nsb-data-tools`; the runtime library performs no catalogue downloads.

## License

AGPL-3.0-only. Scientific assets and third-party dependencies retain their own
licensing and attribution requirements; see the repository notices and
manifests.
