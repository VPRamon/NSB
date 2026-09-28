# nsb

Night-sky background evaluation and observation planning for ground-based
astronomy.

The `nsb` crate is the scientific runtime used by the NSB project. It provides
typed point evaluation, component models, observing-window planning, scientific
metadata, and optional Python bindings. Command-line parsing and observatory
catalog conveniences live in the sibling `nsb-cli` package.

## Rust example

```rust,no_run
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;

# fn evaluate(time: tempoch::Time<tempoch::UTC>) -> nsb::Result<()> {
let sgr_a_star = Target::new(266.41683 * DEG, -29.00781 * DEG);
let query = PointQuery::new(
    observatories::EL_PARANAL.geodetic(),
    time,
    sgr_a_star,
)
.with_components(ComponentMask::ALL);

let result = NsbEvaluator::new()?.evaluate(&query)?;
println!("{}", result.integrated);
# Ok(())
# }
```

NSB reports integrated photon radiance over 300 to 650 nm together with
component maturity and provenance metadata. Generic and planning models must not
be interpreted as site-calibrated products.

Project documentation, scientific validation, Python usage, and release notes
are maintained in the
[NSB repository](https://github.com/VPRamon/NSB).
