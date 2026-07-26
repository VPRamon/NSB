# User guide

Status: Current user-facing entry point.
Audience: Astronomers, observatory planners, CLI users, and Rust library users.
Scope: What NSB does, which interface to use, and how to interpret results.
Non-goals: Scientific validation evidence and release procedures are documented separately.

## What NSB is

NSB is a Rust library and command-line application for modelling the ground-based
night-sky background and finding observing periods that satisfy an NSB limit.
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
| `nsb-cli` | You need interactive evaluation, scripts, tables, JSON, or CSV. | [Getting started](getting-started.md) |
| `nsb` Rust crate | You are integrating NSB into a Rust application or scheduler. | [Getting started: Rust API](getting-started.md#rust-library) |
| `nsb-data-tools` | You maintain scientific assets or build new starlight products. | [Maintainer guide](../maintainer-guide/README.md) |

## Recommended reading path

1. [Getting started](getting-started.md)
2. [Runtime components](components.md)
3. [Observatory configuration and customisation](observatory-customization.md)
4. [Scientific metadata](../specifications/scientific-metadata.md)
5. [Model maturity](../specifications/model-maturity.md)

For implementation details, continue with the
[developer guide](../developer-guide/README.md). For data generation, validation,
and releases, continue with the [maintainer guide](../maintainer-guide/README.md).

## Important interpretation boundaries

- `--components all` means the complete production-safe component set compiled
  into the current build.
- Production starlight is fail-closed. An incomplete experimental seed is never
  selected as a production fallback.
- Built-in CTAO profiles are explicit planning presets, not validated
  site-calibrated products.
- Output metadata is part of the scientific contract. Do not discard maturity,
  provenance, version, uncertainty, or asset-checksum fields in downstream
  systems.

See the [validation matrix](../specifications/validation.md) before using NSB output as a
scientific reference or operational acceptance criterion.
