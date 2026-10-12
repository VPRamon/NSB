# Siderust compatibility

Status: Siderust 0.14 compatibility baseline for the next NSB release.
Audience: Maintainers, downstream packagers, and dependency reviewers.
Scope: Siderust dependency source, lockfile package identity, update rules, and release
requirements.
Non-goals: This document does not claim a Git revision for registry dependencies.

## Current State

| NSB | Siderust package | Manifest source | Public source identity | Rust MSRV | Status |
| --- | --- | --- | --- | --- | --- |
| Unreleased; release boundary pending | 0.14.0 | crates.io | `crates.io:siderust:0.14.0` | 1.89 | Siderust 0.14 migration |

All three workspace crates require the published Rust package with `siderust = "0.14"`.
The committed lockfile resolves 0.14.0.
`crates/nsb` and `crates/nsb-cli` request the `atmosphere` and
`photometry` features, while `crates/nsb-data-tools` uses its own feature set.
The version/source identity is shared even though feature selection is not
identical.

`Cargo.lock` resolves the registry package to its published checksum.
The workspace also uses Tempoch `0.10.0`, and both Python interoperability
crates (`siderust-py` and `tempoch-py`) use `0.4.0`.

### Numerical baseline

The migration resolves Siderust 0.14.0, Tempoch / tempoch-core 0.10.0,
Principia 0.6.0, and siderust-py / tempoch-py 0.4.0 entirely from crates.io.
Registry availability and Python 0.4.0 Linux/macOS/Windows wheels were checked
on 2026-10-12. No Git patch or override is used. The Rust 1.89 gate remains.

The dependency-only comparison keeps the legacy scanner on both sides:
64 CTAO-N/S, both lunar models, all 16 masks, seven-day cases plus three
UTC/TT/leap-boundary conversion cases. Physical-window endpoints, authoritative
radiances, scanner UTC periods and temporal round trips are identical in all
67 exported records. See [A](adaptive-acceptance/dependency-A.jsonl) and
[B](adaptive-acceptance/dependency-B.jsonl); rerun the ignored
`dependency_upgrade_reference` export before/after a dependency update.
This is finite empirical evidence, not proof for every date/configuration.
The B-versus-C search comparisons are recorded separately in the production
adaptive report. No numerical tolerance or scientific model formula changed.

### Python package versioning

The Python runtime dependency named `siderust` is the Siderust Python
distribution and currently uses the independent `0.4.x` version line:

```text
nsb-rust -> siderust>=0.4.0,<0.5
         -> tempoch>=0.4.0,<0.5
```

This is not a mismatch with the Rust `siderust = "0.14"` dependency. The
Python extension uses `siderust-py` for cross-extension interoperability,
while the Rust scientific implementation links the Rust crate shown in the
matrix above.

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
