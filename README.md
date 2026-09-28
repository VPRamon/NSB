# NSB

[![CI](https://github.com/VPRamon/NSB/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/VPRamon/NSB/actions/workflows/ci.yml)
[![License: AGPL-3.0-only](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](LICENSE)
[![Rust: 1.89+](https://img.shields.io/badge/rust-1.89%2B-orange.svg)](https://www.rust-lang.org/)
[![Python: 3.10+](https://img.shields.io/badge/python-3.10%2B-blue.svg)](https://www.python.org/)

Night-sky background evaluation and observation planning for ground-based
astronomy.

NSB evaluates the optical night-sky background for a specified observatory,
UTC time, and target direction. It can also search a time range for periods that
satisfy an NSB threshold together with Sun and target-altitude constraints.

The runtime composes selected contributions from zodiacal light, airglow,
scattered moonlight, and integrated starlight where an admitted starlight
product is available. Results report integrated photon radiance over 300 to
650 nm in `ph cm^-2 ns^-1 sr^-1` and retain scientific metadata needed to
interpret the calculation.

NSB is available through Python, Rust, and a command-line interface.

## What NSB does

| Capability | Support |
| --- | --- |
| Point NSB evaluation | Python, Rust, CLI |
| Observing-window search | Python, Rust, CLI |
| Zodiacal light, airglow, and scattered moonlight | Runtime components |
| Integrated starlight | Validated bundled or external products, with explicit maturity rules |
| Arbitrary observatory coordinates | Supported |
| Named observatory catalogs | CLI through Siderust `ObservatoryCatalog` |
| Machine-readable output | Versioned JSON and CSV schemas |
| Scientific audit metadata | Provenance, maturity, validated domain, model identity, uncertainty where available |
| Offline evaluation | No runtime catalog downloads or data-generation tools |

## Python quickstart

The Python distribution is named `nsb-rust`; the import name is `nsb`.
Python 3.10 or newer is supported.

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

# CTAO South WGS84 coordinates from the bundled observatory catalog.
ctao_south = nsb.Observer(
    lon_deg=-70.31634444444444,
    lat_deg=-24.683427777777776,
    height_m=2184.6,
)
# Sagittarius A* in ICRS coordinates.
sgr_a_star = nsb.Direction(
    ra_deg=266.41683,
    dec_deg=-29.00781,
)

config = (
    nsb.NsbModelConfig.generic_clear_sky()
    .with_site_profile(nsb.SiteProfile.CTA_SOUTH)
)
evaluator = nsb.NsbEvaluator(config)

result = evaluator.evaluate(
    nsb.PointQuery(
        ctao_south,
        datetime(2026, 9, 27, 22, 0, tzinfo=timezone.utc),
        sgr_a_star,
    )
)

print(result.integrated_photons_cm2_ns_sr)
for component in result.components:
    print(component.name, component.integrated_photons_cm2_ns_sr)
```

The CTAO selector above is a planning preset exposed by the Python application
layer. It is not a claim that CTAO South is site-calibrated.

Search for periods below an NSB threshold:

```python
max_nsb_photons_cm2_ns_sr = 0.25
sample_step_s = 600.0

query = nsb.ThresholdQuery(
    ctao_south,
    sgr_a_star,
    datetime(2026, 9, 27, 20, 0, tzinfo=timezone.utc),
    datetime(2026, 9, 28, 4, 0, tzinfo=timezone.utc),
    max_nsb_photons_cm2_ns_sr,
    sample_step_s=sample_step_s,
)

context = evaluator.prepare_site_window_context(query)
result = evaluator.periods_below_threshold_with_context(context, query)

for start, end in result.periods:
    print(start, end)
```

Python datetime inputs must be timezone-aware. See
[Python bindings](docs/python.md) for the complete API, units, errors, and
development workflow.

## CLI

Evaluate Sagittarius A* from CTAO South:

```bash
cargo run --locked -p nsb-cli -- \
  --format json point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --site-profile cta-south \
  --ra 266.41683 --dec -29.00781
```

Find periods for Sagittarius A* with integrated NSB at or below
`0.25 ph cm^-2 ns^-1 sr^-1`:

```bash
cargo run --locked -p nsb-cli -- \
  --format csv window \
  --start 2026-06-18T20:00:00Z \
  --end 2026-06-19T06:00:00Z \
  --site CTAO-S \
  --site-profile cta-south \
  --ra 266.41683 --dec -29.00781 \
  --max-nsb 0.25 \
  --sun-altitude-max -18 \
  --target-altitude-min 20 \
  --step 600
```

Window search evaluates the authoritative NSB model at the selected sampling
resolution and refines bracketed threshold crossings with that same model. It
does not claim continuous completeness between samples. Select a smaller
`--step` when finer temporal resolution is required.

See [Getting started](docs/user-guide/getting-started.md) for CLI options,
component selection, site lookup, and output formats.

## Rust

```rust,no_run
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;

# fn evaluate(time: tempoch::Time<tempoch::UTC>) -> nsb::Result<()> {
let evaluator = NsbEvaluator::new()?;
// Sagittarius A* in ICRS coordinates.
let sgr_a_star = Target::new(266.41683 * DEG, -29.00781 * DEG);

let query = PointQuery::new(
    observatories::EL_PARANAL.geodetic(),
    time,
    sgr_a_star,
)
.with_components(ComponentMask::ALL);

let result = evaluator.evaluate(&query)?;
println!("{}", result.integrated);
# Ok(())
# }
```

Construct and reuse an evaluator. Immutable model data and runtime assets are
prepared at evaluator construction rather than reparsed for every query.

## Observatory integration

NSB separates the physical observatory location from the scientific assumptions
used to model the sky.

| Concern | Selected by |
| --- | --- |
| Observatory location | Named catalog entry or explicit longitude, latitude, and height |
| Scientific assumptions | Site profile and model configuration |

Adding an observatory to a catalog establishes its location. It does not make
the NSB model calibrated for that site.

The CLI can use Siderust's bundled observatories, NSB catalog extensions, an
external Siderust-format catalog, or explicit coordinates. Without an explicit
site profile, arbitrary coordinates and named observatories use generic
clear-sky assumptions.

The CLI and Python interfaces expose CTAO North and South planning presets.
These are planning assumptions, not calibrated site products.

See [Observatory configuration and customisation](docs/user-guide/observatory-customization.md)
for custom observatories, catalog precedence, external starlight, and the
requirements for adding a calibrated profile.

## Scientific model and maturity

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
          +-> point evaluation
          +-> window planning
```

The current scientific roles are:

| Component | Current role |
| --- | --- |
| Zodiacal light | Generic clear-sky planning model |
| Airglow | Generic model or explicit planning preset |
| Jones 2013 moonlight | Generic model or explicit planning preset |
| KS91 moonlight | Published reference and alternate model |
| Integrated starlight | Production only for a validated, admitted product and its declared domain |

`ComponentMask::ALL`, `ComponentMask::DEFAULT`, and CLI
`--components all` use the same production-safe composition. Production
starlight is included only when a validated bundled product has been admitted.
Experimental starlight is never silently substituted.

B/V magnitude and S10 fields are central-wavelength diagnostics, not validated
Johnson B/V passband integrations.

Scientific maturity is part of the result contract. NSB exposes component
identity, provenance, calibration status, validated domain, uncertainty where
available, selected model information, and runtime asset checksums.

Software correctness and site calibration are separate concerns. The current
generic components and CTAO planning profiles must not be interpreted as
site-calibrated products. The bundled Gaia DR3 starlight candidate also remains
outside the production default until its scientific and redistribution review
gates are satisfied.

Read [Scientific model](docs/specifications/scientific-model.md),
[Model maturity](docs/specifications/model-maturity.md),
[Scientific metadata](docs/specifications/scientific-metadata.md), and the
[Validation matrix](docs/specifications/validation.md) before using results for
scientific claims.

## Reproducibility and automation

Runtime evaluation is local and deterministic for fixed inputs and admitted
assets. Scientific data products are generated and reviewed offline.

The CLI exposes versioned JSON and CSV contracts for schedulers, pipelines, and
archived analyses. Output retains model and asset identity so downstream systems
can preserve the provenance of a calculation.

See [Stable CLI schemas](docs/specifications/cli-schemas.md) and the
[Performance contract](docs/specifications/performance.md).

## Documentation

| Topic | Document |
| --- | --- |
| Python | [Python bindings](docs/python.md) |
| First CLI and Rust workflows | [Getting started](docs/user-guide/getting-started.md) |
| User guide | [User documentation](docs/user-guide/README.md) |
| Observatory integration | [Observatory configuration](docs/user-guide/observatory-customization.md) |
| Runtime components | [Component guide](docs/user-guide/components.md) |
| Scientific model | [Concepts and implementation](docs/specifications/scientific-model.md) |
| Maturity and validation | [Model maturity](docs/specifications/model-maturity.md), [Validation matrix](docs/specifications/validation.md) |
| Machine-readable output | [CLI schemas](docs/specifications/cli-schemas.md) |
| Architecture | [Architecture and modules](docs/developer-guide/architecture.md) |
| Scientific data | [Dataset workflow](docs/maintainer-guide/datasets.md) |
| Full index | [Documentation hub](docs/README.md) |

## License

NSB source is licensed under AGPL-3.0-only; see [`LICENSE`](LICENSE).
Third-party dependencies and scientific assets retain their own license and
attribution requirements.
