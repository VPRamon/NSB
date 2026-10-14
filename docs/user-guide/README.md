# User guide

Status: Current user-facing entry point.
Audience: Astronomers, observatory planners, Python users, CLI users, and Rust library users.
Scope: What NSB does, which interface to use, and how to interpret results.
Non-goals: Scientific validation evidence and release procedures are documented separately.

## What NSB is

NSB is a Rust library with Python bindings and a command-line application for
modelling the ground-based night-sky background and finding observing periods
that satisfy an NSB limit.
It evaluates a configurable sum of physical and empirical components for a
specific observer, UTC time, and sky direction.

The primary result is integrated photon radiance over 300–650 nm in
`ph cm^-2 ns^-1 sr^-1`. NSB also reports the contribution and scientific
metadata of each selected component. B/V magnitudes and S10 values are
central-wavelength diagnostics; they are not validated Johnson passband
integrations.

NSB supports two operational workflows:

1. **Point evaluation** — estimate the NSB at one time and target direction.
2. **Window search** — find UTC periods that satisfy an NSB threshold together
   with optional Sun-altitude and target-altitude constraints.

NSB is designed for deterministic planning and reproducible scientific
integration. Its software quality gates do not imply that every component is
site-calibrated. Always inspect component maturity and provenance in the output.

## Choose an interface

| Interface | Use it when | Main entry point |
| --- | --- | --- |
| `nsb-rust` Python package | You need the NSB evaluator/planner from Python while using canonical Siderust coordinate types. | [Python bindings](../python.md) |
| `nsb-cli` | You need interactive evaluation, scripts, tables, JSON, or CSV. | [Getting started](getting-started.md) |
| `nsb` Rust crate | You are integrating NSB into a Rust application or scheduler. | [Getting started: Rust API](getting-started.md#rust-library) |
| `nsb-data-tools` | You maintain scientific assets or build new starlight products. | [Maintainer guide](../maintainer-guide/README.md) |

## Recommended reading path

1. Choose the interface above; Python users should start with [Python bindings](../python.md).
2. [Getting started](getting-started.md) for CLI/Rust workflows
3. [Runtime components](components.md)
4. [Observatory configuration and customisation](observatory-customization.md)
5. [Scientific metadata](../specifications/scientific-metadata.md)
6. [Model maturity](../specifications/model-maturity.md)

For implementation details, continue with the
[developer guide](../developer-guide/README.md). For data generation, validation,
and releases, continue with the [maintainer guide](../maintainer-guide/README.md).

## Important interpretation boundaries

- `--components all` means the complete production-safe component set compiled
  into the current build.
- Production starlight is fail-closed. Missing or invalid production evidence is
  an error; there is no bundled experimental fallback.
- Application-layer CTAO planning profiles are explicit planning presets, not
  validated site-calibrated products.
- Output metadata is part of the scientific contract. Do not discard maturity,
  provenance, version, uncertainty, or asset-checksum fields in downstream
  systems.

See the [validation matrix](../specifications/validation.md) before using NSB output as a
scientific reference or operational acceptance criterion.

## Scientific data and licensing

NSB **code** is AGPL-3.0-only; that does not make all scientific data AGPL.
PALACE and selected reference products use CC BY 4.0, NOAA products have their
applicable federal-work/data conditions, and Gaia-derived Starlight retains
applicable **CC BY-NC 3.0 IGO** terms. The code may be used commercially under
AGPL's obligations; commercial use of protected bundled data may need separate
rights. Credits do not imply endorsement by ESA, DPAC, CDS or the authors.

The compact Starlight release architecture is implemented and tested offline,
but bundled production activation remains gated by the actual #103 human
review. Before that approval, the standard default has three components;
authorized promotion makes the four-component default available in Rust/Python.

See the [source terms, attribution, public contact directory and concern procedure](../nsb_components/starlight/licensing/README.md)
and the distributed `THIRD_PARTY_NOTICES.md`. Cite the original scientific
sources, preserve their notices and assess the data terms for your actual use.
