# Maintainer guide

Status: Current operational and release-maintenance entry point.
Audience: Release maintainers, scientific-data maintainers, and reviewers.
Scope: Data updates, data products, tools, release gates, compatibility, and evidence management.

Maintainers are responsible for two independent quality surfaces:

1. **software release quality** — builds, tests, schemas, compatibility,
   deterministic behaviour, licensing, and packaging;
2. **scientific product maturity** — provenance, calibration, validation,
   uncertainty, completeness, and the claims allowed for each component or asset.

A successful software release must not silently promote an experimental or
planning product to calibrated science.

## Maintainer reading path

1. [Updating scientific data](updating-data.md)
2. [Data-product workflow](data-products.md)
3. [Data-tool reference](tools.md)
4. [Data-product pipeline architecture](../specifications/data-product-pipeline.md)
5. [Validation matrix](../specifications/validation.md)
6. [Release checklist](../operations/release-checklist.md)

For starlight work, continue with:

- [Starlight science requirements](../nsb_components/starlight/science-requirements.md)
- [Starlight generation](../nsb_components/starlight/map-generation.md)
- [Starlight validation](../nsb_components/starlight/map-validation.md)
- [External starlight manifest](../nsb_components/starlight/external-manifest.md)

For implementation ownership, use the
[complete module reference](../developer-guide/module-reference.md).

## Change classification

| Change type | Minimum review surfaces |
| --- | --- |
| Runtime code only | Build, formatting, Clippy, unit/integration/doc tests, performance impact, public API docs |
| CLI argument or output | CLI smoke tests, schema compatibility, user documentation, logging behaviour |
| Scientific model | Scientific contract, metadata, uncertainty, validation fixtures, model maturity, performance |
| Runtime asset | Data-update runbook, manifest coverage, checksum, provenance, license, schema, validation, build-time admission |
| Data-product tool | Cargo manifest, tool registry, thin-adapter rule, tool reference, inputs/outputs, resume semantics, exit codes, tests |
| Persisted schema or checkpoint | Explicit schema version, rejection of unknown/incompatible data, migration/recovery tests |
| Dependency update | Lockfile, MSRV, compatibility policy, licensing, reproducibility metadata |

## Production principles

- Candidate generation and production admission are separate operations.
- Every required production gate must be executed and pass.
- `NotRun` is not equivalent to `Passed`.
- Production workflows fail closed on missing provenance, checksums, validation,
  or inconsistent source accounting.
- Resume may reuse only verified state with a documented recovery action.
- Generated data and reports belong in caller-selected output directories.
- Source control contains contracts, fixtures, policies, and reviewed runtime
  assets—not ad hoc machine outputs.
- Experimental and migration-only tools must remain visibly classified and must
  not be presented as production entry points.

## Release gates

The standard workspace gates are:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
cargo deny check
```

Run the asset and tool inventories independently:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- assets verify \
  --manifest crates/nsb/data/manifest.toml
cargo test --locked -p nsb-data-tools --test tool_registry
```

Use the full [Release checklist](../operations/release-checklist.md) before tagging or
distributing a release.

## Authoritative records

| Record | Authority |
| --- | --- |
| Runtime asset inventory | `crates/nsb/data/manifest.toml` |
| Data-tool inventory and contracts | `crates/nsb-data-tools/tool-registry.toml` |
| Module ownership and intent | `docs/developer-guide/module-reference.md` plus rustdoc |
| Data-update procedure | `docs/maintainer-guide/updating-data.md` |
| Public machine output | Versioned JSON/CSV schema documentation and tests |
| Scientific maturity | Runtime metadata plus `docs/specifications/model-maturity.md` |
| Scientific evidence | Validation fixtures, reports, and `docs/specifications/validation.md` |
| Dependency identity | Cargo manifests, lockfile, and compatibility documentation |
| Release history | `CHANGELOG.md` and immutable tags |
