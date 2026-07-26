# Developer guide

Status: Current contributor entry point.
Audience: Contributors extending runtime models, CLI behaviour, or data-product services.
Scope: Repository architecture, module ownership, development workflow, and design constraints.

## Repository architecture

NSB is a Cargo workspace with deliberately separated responsibilities:

| Crate | Responsibility | Must not own |
| --- | --- | --- |
| `nsb` | Typed scientific runtime API, component composition, point evaluation, threshold-window search, runtime assets, and scientific metadata | CLI parsing, named operational aliases, output formatting, catalogue downloads, or release orchestration |
| `nsb-cli` | Command-line parsing, named site aliases, timestamp and coordinate parsing, stable JSON/CSV/table presentation, and operational logging | Scientific algorithms or offline data generation |
| `nsb-data-tools` | Offline acquisition, transformation, validation, reconciliation, and packaging of scientific data products | Runtime query behaviour or an alternative CLI model implementation |

Read [Architecture and modules](architecture.md) for system flow and design
boundaries. Use the [Module reference](module-reference.md) to locate every
crate-level module and its ownership.

## Development setup

```bash
cargo build --workspace --locked
cargo test --workspace --locked
cargo test --workspace --doc --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

The minimum supported Rust version is 1.89. Source code forbids unsafe code in
the principal library crates.

## Where to make a change

| Change | Primary location | Required documentation |
| --- | --- | --- |
| New or modified physical NSB component | `crates/nsb/src/components/` | User component overview, maturity, validation evidence, module reference, and Rust API docs |
| Point or window orchestration | `crates/nsb/src/evaluator/` and `window_search` | Architecture, performance contract, module reference, and tests |
| New site profile or calibration | `crates/nsb/src/site.rs` | Observatory customisation, site assumptions, maturity, and validation |
| CLI argument, command, or output | `crates/nsb-cli/src/` | Getting started and CLI schema when machine output changes |
| Scientific asset | `crates/nsb/data/` and its manifest | Provenance, checksum, validation, data-update runbook, and release impact |
| Dataset command | thin `nsb-data` adapter plus typed dataset engine | Cargo manifest, versioned configuration, run manifest, validation report, and exit-code contract |
| Persisted pipeline schema | `crates/nsb-data-tools/src/platform/pipeline/` | Architecture, module reference, migration policy, recovery and contract tests |

## Core design rules

1. Keep scientific quantities typed. Avoid raw unitless values at public and
   cross-module boundaries when an existing quantity type is available.
2. Preserve component separation. Totals must remain traceable to individual
   contributions and their metadata.
3. Keep executable adapters thin. Reusable behaviour belongs in library modules
   or services, not `src/bin`.
4. Treat scientific maturity as data. Do not infer calibration from passing
   software tests.
5. Fail closed for production assets and admission gates.
6. Keep output schemas versioned and deterministic.
7. Register immutable runtime assets with checksums, provenance, license, and
   maturity metadata.
8. Do not duplicate astronomy primitives already owned by Siderust.

## Testing expectations

A change normally needs tests at more than one level:

- unit tests for scientific or parsing behaviour;
- contract tests for schemas, checksums, metadata, and fail-closed rules;
- integration tests for public API or CLI behaviour;
- validation fixtures for scientific claims;
- benchmarks when point or threshold-window performance can change.

Read the [Validation matrix](../specifications/validation.md),
[Performance contract](../specifications/performance.md), and
[Logging contract](../specifications/logging.md) before changing those surfaces.

## Documentation expectations

Public Rust APIs require rustdoc. User workflows belong in `docs/user-guide/`.
Implementation, module ownership, and extension guidance belong in
`docs/developer-guide/`. Release, data-product, and operational procedures belong
in `docs/maintainer-guide/`. Scientific evidence and stable contracts may remain
as specialised reference documents linked from those guides.

When a change affects more than one audience, update the authoritative technical
reference first, then update the relevant audience-facing overview without
copying mutable details into several places.
