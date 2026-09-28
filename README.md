# NSB

Night-sky background evaluation and observation planning for ground-based astronomy.

NSB evaluates the optical night-sky background for a specified observatory,
UTC time, and target direction. It can also search a time range for observing
periods that satisfy an NSB threshold together with Sun and target-altitude
constraints.

The runtime composes selected contributions from zodiacal light, airglow,
scattered moonlight, and integrated starlight where an admitted starlight
product is available. The primary result is integrated photon radiance over
300 to 650 nm in `ph cm^-2 ns^-1 sr^-1`, with scientific metadata retained
alongside the result.

NSB is available through Python, Rust, and a command-line interface.

## What NSB does

A planning workflow can ask two direct questions:

1. What night-sky background is expected for this target at this site and time?
2. When does the target satisfy a requested NSB threshold and observing constraints?

| Capability | Current interface |
| --- | --- |
| Point NSB evaluation | Python, Rust, CLI |
| Observing-window search | Python, Rust, CLI |
| Zodiacal light | Runtime component |
| Airglow | Runtime component |
| Scattered moonlight | Runtime component |
| Integrated starlight | Validated bundled or external products, with explicit maturity rules |
| Arbitrary observatory coordinates | Rust and CLI, with Python observer coordinates |
| Named observatory catalogs | CLI through Siderust `ObservatoryCatalog` |
| Machine-readable output | Versioned JSON and CSV schemas |
| Scientific provenance | Component maturity, provenance, validated domain, model selection, uncertainty where available |
| Runtime asset identity | Checksums and manifest metadata for admitted assets |
| Offline evaluation | Runtime evaluation does not download catalogs or invoke data-generation tools |

## Python

The Python distribution is named `nsb-rust`; the import name is `nsb`.
Python 3.10 or newer is supported through the CPython stable ABI.

From a repository checkout:

```bash
python -m venv .venv
. .venv/bin/activate
python -m pip install -U pip "maturin>=1.9,<2"
maturin develop --locked
```

Evaluate one target:

```python
from datetime import datetime, timezone

import nsb

observer = nsb.Observer(-70.4, -24.6, 2600.0)
direction = nsb.Direction(266.4, -29.0)

config = (
    nsb.NsbModelConfig.generic_clear_sky()
    .with_site_profile(nsb.SiteProfile.CTA_SOUTH)
)
evaluator = nsb.NsbEvaluator(config)

query = nsb.PointQuery(
    observer,
    datetime(2026, 9, 27, 22, 0, tzinfo=timezone.utc),
    direction,
)
result = evaluator.evaluate(query)

print(result.integrated_photons_cm2_ns_sr)
for component in result.components:
    print(component.name, component.integrated_photons_cm2_ns_sr)
```

The CTAO selector above is a planning preset exposed by the Python application
layer. It is not a claim that CTAO South is site-calibrated.

Search for periods below an NSB threshold:

```python
query = nsb.ThresholdQuery(
    observer,
    direction,
    datetime(2026, 9, 27, 20, 0, tzinfo=timezone.utc),
    datetime(2026, 9, 28, 4, 0, tzinfo=timezone.utc),
    0.25,
    sample_step_s=600.0,
)

context = evaluator.prepare_site_window_context(query)
result = evaluator.periods_below_threshold_with_context(context, query)

for start, end in result.periods:
    print(start, end)
```

Python datetime inputs must be timezone-aware. Non-UTC offsets are normalized
to UTC before evaluation.

See [Python bindings](docs/python.md) for the supported API, units, errors, and
development workflow.

## Command line

Evaluate one target at one instant:

```bash
cargo run --locked -p nsb-cli -- \
  --format json \
  point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --site-profile cta-south \
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
  --site-profile cta-south \
  --ra 83.6331 \
  --dec 22.0145 \
  --max-nsb 0.25 \
  --sun-altitude-max -18 \
  --target-altitude-min 20 \
  --step 600
```

The window search evaluates the authoritative NSB model at the selected sampling
resolution and refines bracketed threshold crossings with that same model. It
does not claim continuous completeness between samples. Select a smaller
`--step` when the science case requires finer temporal resolution.

Global options such as `--format`, `--log-level`, and `-v` precede the
subcommand. See [Getting started](docs/user-guide/getting-started.md) for the
complete CLI workflow.

## Rust library

```rust,no_run
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;

# fn evaluate(time: tempoch::Time<tempoch::UTC>) -> nsb::Result<()> {
let evaluator = NsbEvaluator::new()?;
let result = evaluator.evaluate(
    &PointQuery::new(
        observatories::EL_PARANAL.geodetic(),
        time,
        Target::new(266.41683 * DEG, -29.00781 * DEG),
    )
    .with_components(ComponentMask::ALL),
)?;

println!("total: {}", result.integrated);
for component in result.components {
    println!("{}: {}", component.name, component.integrated);
}
# Ok(())
# }
```

Construct and reuse an evaluator. Immutable model data and runtime assets are
prepared when the evaluator is created rather than reparsed for every query.

## Observatory integration

NSB separates the physical observatory location from the scientific assumptions
used to model the sky.

| Concern | Selected by |
| --- | --- |
| Observatory location | Named catalog entry or explicit longitude, latitude, and height |
| Scientific assumptions | Site profile and model configuration |

This distinction is deliberate. Adding an observatory to a catalog establishes
its location; it does not make the NSB model calibrated for that site.

The CLI can use Siderust's bundled observatories, NSB catalog extensions, an
external Siderust-format observatory catalog, or explicit coordinates:

```bash
nsb point \
  --time 2026-06-18T23:00:00Z \
  --lon 12.5 \
  --lat 41.9 \
  --height 800 \
  --ra 83.6331 \
  --dec 22.0145
```

Without an explicit site profile, arbitrary coordinates and named observatories
use generic clear-sky assumptions. The CLI and Python interfaces expose CTAO
North and South planning presets, but those presets are not calibrated site
products.

See [Observatory configuration and customisation](docs/user-guide/observatory-customization.md)
for catalog precedence, custom observatories, validated external starlight, and
the requirements for adding a calibrated profile.

## Scientific model

NSB composes physical and empirical contributions without hiding the individual
component results:

```text
observer + time + target
          |
          v
  zodiacal light
  airglow
  scattered moonlight
  admitted starlight
          |
          v
 integrated NSB
          |
          +-> point result
          |
          +-> threshold-window planning
```

The current component roles are:

| Component | Default implementation | Scientific role |
| --- | --- | --- |
| Zodiacal light | Leinert 1998 source model with independent Noll-style atmospheric propagation by default | Generic clear-sky planning model |
| Airglow | Empirical continuum with seasonal, nightly, solar-activity, and viewing-geometry terms | Generic model or planning preset |
| Moonlight | Jones et al. 2013 spectral model | Generic model or planning preset |
| KS91 moonlight | Published analytic V-band implementation | Reference and alternate model |
| Integrated starlight | Validated HEALPix product when explicitly admitted | Production only for the domain justified by its evidence |

`ComponentMask::ALL`, `ComponentMask::DEFAULT`, and CLI
`--components all` share the same production-safe composition. A production
starlight component is included only when a validated bundled product has been
admitted. Experimental starlight is never silently substituted.

B/V magnitude and S10 fields are central-wavelength diagnostics, not validated
Johnson B/V passband integrations.

See [Scientific model](docs/specifications/scientific-model.md) for the model
contract and component composition.

## Scientific transparency

Scientific maturity is part of the result contract, not only a documentation
label. NSB records information needed to interpret and audit a result, including:

- component identity and selected model
- calibration or maturity status
- provenance and validated domain
- uncertainty where supported by the component
- site-profile identity
- runtime asset checksums
- Airglow geometry and solar-activity provenance where applicable

Software correctness and site calibration are separate concerns. The current
generic components and CTAO planning profiles must not be interpreted as
site-calibrated products.

The bundled Gaia DR3 starlight candidate remains outside the production default
until the repository's scientific and redistribution review gates are satisfied.
Validated external starlight can be admitted through its documented sidecar
contract for the domain justified by the supplied evidence.

Read [Model maturity](docs/specifications/model-maturity.md),
[Scientific metadata](docs/specifications/scientific-metadata.md), and the
[Validation matrix](docs/specifications/validation.md) before using results for
scientific claims.

## Reproducibility and automation

Runtime evaluation is local and deterministic for fixed inputs and admitted
assets. Scientific data products are generated and reviewed offline. Runtime
code does not download catalogs or invoke data-generation tools.

The CLI exposes versioned JSON and CSV contracts intended for schedulers,
pipelines, and archived analyses. Output includes model and asset identity so
downstream systems can retain the provenance of a calculation.

See [Stable CLI schemas](docs/specifications/cli-schemas.md).

## Performance

NSB reuses immutable evaluator state and supports reusable site-window contexts
for planning workloads. The current performance specification records a
representative one-year CTAO South search with all default components, a
600-second sampling step, and two Rayon threads at a median 3.27 seconds on an
AMD Ryzen 7 5700X test host.

That measurement is review evidence for a documented workload, not a portable
runtime guarantee. See [Performance contract](docs/specifications/performance.md)
for the hardware, commands, correctness constraints, and full benchmark matrix.

## Documentation

| Topic | Document |
| --- | --- |
| First CLI and Rust workflows | [Getting started](docs/user-guide/getting-started.md) |
| Python API | [Python bindings](docs/python.md) |
| User documentation | [User guide](docs/user-guide/README.md) |
| Observatory integration | [Observatory configuration](docs/user-guide/observatory-customization.md) |
| Runtime components | [Runtime components](docs/user-guide/components.md) |
| Scientific model | [Scientific model](docs/specifications/scientific-model.md) |
| Model maturity | [Model maturity](docs/specifications/model-maturity.md) |
| Validation evidence and gaps | [Validation matrix](docs/specifications/validation.md) |
| Scientific metadata | [Scientific metadata](docs/specifications/scientific-metadata.md) |
| Machine-readable output | [CLI schemas](docs/specifications/cli-schemas.md) |
| Architecture | [Architecture and modules](docs/developer-guide/architecture.md) |
| Dataset workflow | [Dataset workflow](docs/maintainer-guide/datasets.md) |
| Documentation index | [Documentation hub](docs/README.md) |

## Repository layout

| Crate | Purpose |
| --- | --- |
| [`nsb`](crates/nsb) | Scientific runtime library, point evaluation, planning, runtime assets, and Python bindings |
| [`nsb-cli`](crates/nsb-cli) | Operational CLI, observatory selection, output schemas, and logging |
| [`nsb-data-tools`](crates/nsb-data-tools) | Offline acquisition, validation, reconciliation, and packaging of scientific data products |

Siderust owns general astronomy primitives such as time, coordinates, ephemerides,
events, atmosphere, passbands, HEALPix, and observatory catalogs. NSB owns
night-sky component composition, NSB-specific empirical data, observing-window
planning, and maturity-bearing metadata.

See [Architecture and modules](docs/developer-guide/architecture.md) for the
full ownership and extension model.

## Development

The minimum supported Rust version is 1.89.

Useful repository checks include:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Python binding development and installed-wheel validation are documented in
[docs/python.md](docs/python.md).

## License

NSB source is licensed under AGPL-3.0-only; see [`LICENSE`](LICENSE).
Third-party dependencies and scientific assets retain their own license and
attribution requirements.
