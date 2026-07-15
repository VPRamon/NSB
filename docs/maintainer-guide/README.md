# Maintainer guide

Status: Current operational and release-maintenance entry point.
Audience: Release maintainers, scientific-data maintainers, and reviewers.
Scope: Data products, tools, release gates, compatibility, and evidence management.

Maintainers are responsible for two independent quality surfaces:

1. **software release quality** — builds, tests, schemas, compatibility,
   deterministic behaviour, licensing, and packaging;
2. **scientific product maturity** — provenance, calibration, validation,
   uncertainty, completeness, and the claims allowed for each component or asset.

A successful software release must not silently promote an experimental or
planning product to calibrated science.

## Maintainer reading path

1. [Data-product workflow](data-products.md)
2. [Data-tool reference](tools.md)
3. [Data-product pipeline architecture](../DATA_PRODUCT_PIPELINE_ARCHITECTURE.md)
4. [Validation matrix](../VALIDATION.md)
5. [Release checklist](../RELEASE_CHECKLIST.md)

For starlight work, continue with:

- [Starlight science requirements](../STELLAR_MAP_SCIENCE_REQUIREMENTS.md)
- [Starlight generation](../STELLAR_MAP_GENERATION.md)
- [Starlight validation](../STELLAR_MAP_VALIDATION.md)
- [External starlight manifest](../EXTERNAL_STARLIGHT_MANIFEST.md)

## Change classification

| Change type | Minimum review surfaces |
| --- | --- |
| Runtime code only | Build, formatting, Clippy, unit/integration/doc tests, performance impact, public API docs |
| CLI argument or output | CLI smoke tests, schema compatibility, user documentation, logging behaviour |
| Scientific model | Scientific contract, metadata, uncertainty, validation fixtures, model maturity, performance |
| Runtime asset | Manifest coverage, checksum, provenance, license, schema, validation, build-time admission |
| Data-product tool | Tool registry, thin-adapter rule, input/output contract, resume semantics, exit codes, tests |
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

Run the asset registry check independently:

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

Use the full [Release checklist](../RELEASE_CHECKLIST.md) before tagging or
distributing a release.

## Authoritative records

| Record | Authority |
| --- | --- |
| Runtime asset inventory | `crates/nsb/data/manifest.toml` |
| Data-tool inventory and contracts | `crates/nsb-data-tools/tool-registry.toml` |
| Public machine output | Versioned JSON/CSV schema documentation and tests |
| Scientific maturity | Runtime metadata plus `MODEL_MATURITY.md` |
| Scientific evidence | Validation fixtures, reports, and `VALIDATION.md` |
| Dependency identity | Cargo manifests, lockfile, and compatibility documentation |
| Release history | `CHANGELOG.md` and immutable tags |

## Historical and audit material

Audits, migration reports, uncertainty studies, and duplication registers are
valuable evidence but should not be used as primary user instructions. Link them
from the relevant maintainer or scientific reference page, mark their date and
status clearly, and avoid copying historical commands into current workflows.