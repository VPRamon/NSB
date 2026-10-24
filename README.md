# NSB

[![CI](https://github.com/VPRamon/NSB/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/VPRamon/NSB/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/nsb.svg?label=crates.io)](https://crates.io/crates/nsb)
[![PyPI](https://img.shields.io/pypi/v/nsb-py.svg?label=PyPI)](https://pypi.org/project/nsb-py/)
[![docs.rs](https://docs.rs/nsb/badge.svg)](https://docs.rs/nsb)
[![Rust 1.89+](https://img.shields.io/badge/Rust-1.89%2B-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0-only](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](LICENSE)

**Night-sky background evaluation and observation planning for ground-based astronomy.**

NSB estimates the optical night-sky background at an observatory, UTC time,
and target direction. It also finds observing windows that satisfy sky-brightness,
Sun-altitude, and target-altitude constraints. Use the same Rust-powered scientific
engine from **Rust**, **Python**, or the **command line**.

<p align="center">
  <a href="https://github.com/VPRamon/NSB/pull/212#issuecomment-6014811662">
    <img src="https://github.com/user-attachments/assets/485394ae-8629-4851-ba57-50daadb436a1" width="860" alt="Diagnostic all-sky integrated starlight heatmap in Galactic Mollweide projection">
  </a>
</p>
<p align="center"><sub>
  Diagnostic Galactic starlight heatmap from <a href="https://github.com/VPRamon/NSB/pull/212">PR #212</a>.
  This visualizes a starlight data product, <strong>not</strong> a total NSB prediction.
</sub></p>

## Observational validation notebooks

Executable notebooks live under [`notebooks/`](notebooks/README.md):

- [`notebooks/NSB_Validation.ipynb`](notebooks/NSB_Validation.ipynb) — standalone Patat/FORS1 campaign
- [`notebooks/NSB_CTAO_Validation.ipynb`](notebooks/NSB_CTAO_Validation.ipynb) — CTAO four-component package (`./ctao/reproduce.sh`)

**v0.2.0 Automatic Airglow is PALACE Oxygen.** Explicit continuum-only selection remains available. This is a release-policy reference-model choice, not an independent absolute observational admission.

## Install

NSB **`0.2.0`** is a release candidate using published Rust/Python dependencies.
Publication still requires maintainer approval. The corrected Siderust Python
bridge/runtime 0.5.1 is required; affected 0.5.0 is excluded.
The latest published NSB release is `0.1.1`; commands targeting `0.2.0` apply
once it is published. Build this checkout to test the candidate.

Choose the interface that fits your workflow:

| Interface | Installation | Requirements |
| --- | --- | --- |
| **Python** | `python -m pip install nsb-py` | Python 3.10+ |
| **Rust** | `cargo add nsb` | Rust 1.89+ |
| **CLI** | Build from the repository (below) | Rust 1.89+ |

For reproducible applications, pin the `0.1.1` release with
`cargo add nsb@0.1.1` or `python -m pip install nsb-py==0.1.1`.

The PyPI **distribution** is `nsb-py`, but its **import** is `nsb`.
The crates.io **library** is `nsb`. The executable is also named `nsb`,
but its workspace crate, `nsb-cli`, is **not** separately published to crates.io.

Install the CLI from the repository:

```bash
git clone https://github.com/VPRamon/NSB.git
cd NSB
cargo install --path crates/nsb-cli --locked
nsb --help
```

The Rust example below also uses Siderust, Tempoch and Chrono at the API
boundary; add the matching dependencies to your application:

```bash
cargo add nsb@0.2.0 siderust@0.15.1 tempoch@0.11.1 chrono@0.4
```

## Quickstart

All examples use **Sagittarius A\*** (RA 266.41683°, Dec −29.00781°).
Point evaluations return integrated photon radiance over **300–650 nm**
in `ph cm^-2 ns^-1 sr^-1`.

### Python

```python
from datetime import datetime, timezone

import nsb
import siderust

observer = siderust.Observer(
    lon_deg=-70.31634, lat_deg=-24.683427, height_m=2184.6
)
target = siderust.Direction(ra_deg=266.41683, dec_deg=-29.00781)

evaluator = nsb.NsbEvaluator()
result = evaluator.evaluate(
    nsb.PointQuery(observer, datetime(2026, 6, 18, 23, tzinfo=timezone.utc), target)
)
print(f"NSB: {result.integrated_photons_cm2_ns_sr:.3e} ph/(cm² ns sr)")
```

This example uses CTAO South's coordinates with the **generic clear-sky**
model, not a site-calibrated CTAO model. Python input times must be
timezone-aware. For component breakdowns, planning and site profiles, see the
[Python guide](docs/python.md).

### Rust

```rust
use chrono::{DateTime, Utc};
use nsb::{NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;
use tempoch::{Time, UTC};

fn main() -> nsb::Result<()> {
    let utc = DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let query = PointQuery::new(
        observatories::EL_PARANAL.geodetic(),
        Time::<UTC>::from_chrono(utc),
        Target::new(266.41683 * DEG, -29.00781 * DEG),
    );

    let result = NsbEvaluator::new()?.evaluate(&query)?;
    println!("NSB: {:.3e} ph/(cm² ns sr)", result.integrated.value());
    Ok(())
}
```

The equivalent, CI-compiled example can be run from a checkout:

```bash
cargo run --locked -p nsb --example point_query
```

See [more Rust examples](examples/README.md) and the [Rust API documentation](https://docs.rs/nsb).
NSB uses Siderust and Tempoch types at its public Rust boundaries.

### CLI

After installing the executable above, evaluate the same target from CTAO South:

```bash
nsb --format table point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S --site-profile cta-south \
  --ra 266.41683 --dec -29.00781
```

Use `--format json` or `--format csv` for machine-readable output.
For observing-window searches (`nsb window`), named observatories and advanced
options, follow the [CLI getting-started guide](docs/user-guide/getting-started.md).

## What can NSB do?

| Capability | What you get |
| --- | --- |
| **Evaluate sky background** | Integrated radiance and per-component contributions for a target, site and UTC instant |
| **Plan observing windows** | Periods satisfying an NSB threshold and optional Sun/target-altitude limits |
| **Choose sky models** | Zodiacal light, airglow, scattered moonlight and admitted integrated starlight products |
| **Integrate with pipelines** | Stable, versioned CLI JSON/CSV schemas and reusable library APIs |
| **Inspect scientific provenance** | Model identity, component maturity, validated domain and uncertainty where available |
| **Run offline** | Local deterministic evaluation for fixed inputs and admitted scientific assets |

NSB deliberately distinguishes **observatory coordinates** from **scientific
site assumptions**. Generic clear-sky and CTAO planning presets are **not
site-calibrated predictions**. Starlight inclusion follows validation and
admission rules; an experimental candidate is never silently substituted.
Reported B/V magnitude and S10 values are central-wavelength diagnostics,
not validated Johnson B/V passband integrations.

For scientific use, review [model maturity](docs/specifications/model-maturity.md),
[validation](docs/specifications/validation.md) and
[scientific metadata](docs/specifications/scientific-metadata.md).

## Documentation and examples

| Start here | Covers |
| --- | --- |
| [Examples index](examples/README.md) | Runnable Rust examples, Python and CLI guides |
| [Python bindings](docs/python.md) | Installation, point evaluation, window planning and API types |
| [Getting started](docs/user-guide/getting-started.md) | CLI usage, site selection and first Rust workflows |
| [Observatory configuration](docs/user-guide/observatory-customization.md) | Named sites, custom coordinates and site profiles |
| [Scientific model](docs/specifications/scientific-model.md) | Physical contributions and modeling assumptions |
| [CLI output schemas](docs/specifications/cli-schemas.md) | Versioned JSON/CSV for automation |
| [Documentation hub](docs/README.md) | Full user, developer and maintainer documentation |

## Releases

The source of truth for the version is
[`crates/nsb/Cargo.toml`](crates/nsb/Cargo.toml); Maturin derives the Python
package version from that same manifest. The
[tag-triggered release workflow](.github/workflows/publish.yml) builds and
checks the Rust crate, Python wheels and source distribution before uploading
anything.
The CLI remains a source-installed workspace application in `0.2.0`.

## License and scientific data

NSB **source code** is licensed under
[AGPL-3.0-only](LICENSE). Scientific datasets and third-party dependencies
retain their own licensing and attribution requirements; the code license
does **not** grant rights to all underlying data.

Some reference products are CC BY 4.0, NOAA data follows its applicable
conditions, and Gaia-derived starlight retains applicable **CC BY-NC 3.0 IGO**
terms. Commercial use of protected scientific data may require separate
permissions. Credits do not imply endorsement by ESA, DPAC, CDS or the
scientific authors.

Read the [Starlight source terms and attribution](docs/nsb_components/starlight/licensing/README.md)
and [third-party notices](THIRD_PARTY_NOTICES.md) before redistributing data
or using model outputs for scientific claims.

### Experimental single scattering

`nsb::transport::single::single_scattering` provides a separate opt-in scalar
reference for synthetic wavelength-resolved TOA illumination (360–570 nm,
output zenith 0–80°). Direct and first-order spectral photon radiance are separate;
higher orders are uncomputed. Evaluator/component defaults do not use this solver.
Run `cargo run -p nsb --example single_scattering` from the repository.

`transport::single::starlight` adapts checksum-validated native spectral bins
with explicit flat photon density, local site/TT/UT1 geometry and independently
refined full-bin wavelength integration. Run the offline `starlight_transport`
example. A conservative HEALPix subpixel path resolves the frozen Gaia angular
case; the labelled historical comparison retains explicit astrometric differences. See the [spectral contract](docs/specifications/starlight-transport.md)
for the bounded numerical method and diagnostic limitations.

The transport APIs use published Siderust 0.15.1 with the coordinated Python
0.5 bridges. This remains an experimental physical reference, with no empirical
sky-accuracy or complete transport claim.
