# Siderust compatibility

Status: Current dependency record for the release branch.
Audience: Maintainers, downstream packagers, and dependency reviewers.
Scope: Siderust dependency source, lockfile package identity, update rules, and release
requirements.
Non-goals: This document does not claim a Git revision for registry dependencies.

## Current State

| NSB | Siderust package | Manifest source | Public source identity | Rust MSRV | Status |
| --- | --- | --- | --- | --- | --- |
| 0.1.x | 0.11.0 | crates.io registry | `crates.io:siderust:0.11.0` | 1.89 | Locked registry dependency |

All three workspace crates currently declare Siderust from the same crates.io
package release:

```toml
siderust = { version = "0.11.0", features = ["atmosphere", "photometry"] }
```

`Cargo.lock` records the resolved crates.io package version and checksum.
Release documentation and CLI metadata must use the source identity above and
must not invent a Git revision for this dependency.

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
starlight-map validation helpers. A Siderust update can change scientific
outputs without changing NSB public API shapes.

## Update Procedure

1. Review upstream Siderust release notes and relevant scientific/data API
   changes.
2. Choose a released Siderust version or exact Git commit for release builds.
3. Keep `crates/nsb`, `crates/nsb-cli`, and `crates/nsb-data-tools` on the same
   dependency source.
4. Run `cargo update -p siderust` and review `Cargo.lock`.
5. Rerun release gates, scientific validation tests, starlight admission tests,
   and CLI schema tests.
6. Update this matrix, root README dependency text, CLI version metadata, and
   `CHANGELOG.md`.
