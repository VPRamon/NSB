# Siderust compatibility

Status: Siderust 0.12 compatibility baseline for the NSB 0.1 release.
Audience: Maintainers, downstream packagers, and dependency reviewers.
Scope: Siderust dependency source, lockfile package identity, update rules, and release
requirements.
Non-goals: This document does not claim a Git revision for registry dependencies.

## Current State

| NSB | Siderust package | Manifest source | Public source identity | Rust MSRV | Status |
| --- | --- | --- | --- | --- | --- |
| 0.1.x | 0.12.0 | crates.io | `crates.io:siderust:0.12.0` | 1.89 | Released baseline |

All three workspace crates declare the published Siderust 0.12 package:

```toml
siderust = { version = "0.12.0", features = ["atmosphere", "photometry"] }
```

`Cargo.lock` resolves the registry package to its published checksum.

Public library exports `nsb::SIDERUST_VERSION` and `nsb::SIDERUST_SOURCE` must
match this matrix. A workspace contract test fails if the declared dependency,
lockfile package, or published provenance constants disagree.


## Release Requirement

A release that claims reproducible dependency resolution must satisfy all of:

1. every workspace manifest uses the same Siderust source;
2. `Cargo.lock` is committed and matches that source;
3. `cargo deny check` passes with the locked graph;
4. CLI version metadata and this matrix report the same dependency identity;
5. Siderust-dependent validation fixtures are rerun.

## Why Updates Need Review

NSB relies on Siderust for time scales, coordinates, ephemerides, atmosphere
primitives, event searches, HEALPix support, Gaia/passband preparation, and
starlight-map validation helpers. The `nsb::transport` layer reuses Siderust
airmass, Rayleigh/Mie optical depth, Beer–Lambert transmission, ozone
transmittance, and phase-function primitives rather than re-implementing them.
A Siderust update can change scientific outputs without changing NSB public API
shapes.

## Update Procedure

1. Review upstream Siderust release notes and relevant scientific/data API
   changes.
2. Choose a released Siderust version or exact Git commit for release builds.
3. Keep `crates/nsb`, `crates/nsb-cli`, and `crates/nsb-data-tools` on the same
   dependency source.
4. Run `cargo update -p siderust` and review `Cargo.lock`.
5. Update `nsb::SIDERUST_VERSION` and `nsb::SIDERUST_SOURCE` to match the
   resolved package (and keep this matrix / CLI expectations in sync).
6. Rerun release gates, scientific validation tests, starlight admission tests,
   and CLI schema tests.
7. Update root README dependency text when present and `CHANGELOG.md`.
