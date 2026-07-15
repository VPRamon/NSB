# `nsb`

Typed Rust runtime library for ground-based night-sky background modelling and
observing-window searches.

This crate owns:

- physical and empirical NSB component models;
- typed observer, UTC time, and target inputs;
- point evaluation and per-component results;
- threshold-window search;
- runtime scientific assets and admission checks;
- model, maturity, provenance, uncertainty, and diagnostic metadata;
- built-in atmospheric and airglow site profiles.

It intentionally does not own command-line parsing, named operational site
aliases, output formatting, catalogue acquisition, or scientific data-product
generation. Those responsibilities belong to `nsb-cli` and `nsb-data-tools`.

## Public module overview

| Module | Purpose |
| --- | --- |
| `assets` | Runtime asset registry and embedded data selection |
| `components` | Zodiacal light, integrated starlight, airglow, and moonlight models |
| `evaluator` | Point queries, threshold searches, model configuration, results, and metadata |
| `site` | Generic and named planning-profile assumptions with explicit calibration status |
| `error` | Typed runtime errors |

## Example

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
# Ok(())
# }
```

Construct and reuse an evaluator so immutable tables and runtime assets are
prepared once.

## Documentation

- [User guide](../../docs/user-guide/README.md)
- [Runtime components](../../docs/user-guide/components.md)
- [Architecture and modules](../../docs/developer-guide/architecture.md)
- [Model maturity](../../docs/MODEL_MATURITY.md)
- [Validation matrix](../../docs/VALIDATION.md)