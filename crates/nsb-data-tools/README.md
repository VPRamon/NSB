# nsb-data-tools

Offline Rust tooling for acquiring, transforming, validating, reconciling, and
packaging NSB scientific data products. Runtime evaluation never invokes this
crate and never downloads catalogues.

## Start here

- [Updating scientific data](../../docs/maintainer-guide/updating-data.md)
- [Complete data-tool reference](../../docs/maintainer-guide/tools.md)
- [Data-product workflow](../../docs/maintainer-guide/data-products.md)
- [Data-product pipeline architecture](../../docs/DATA_PRODUCT_PIPELINE_ARCHITECTURE.md)
- [Starlight generation](../../docs/STELLAR_MAP_GENERATION.md)
- [Starlight validation](../../docs/STELLAR_MAP_VALIDATION.md)

The normative command inventory is [`tool-registry.toml`](tool-registry.toml).
Every compiled binary in `Cargo.toml` must be registered exactly once with:

- owner and intended audience;
- maturity/status;
- purpose;
- input and output contracts;
- resume or idempotency semantics;
- exit-code contract;
- documentation anchor.

## Architecture

```text
src/bin/*
  -> tool_services/*
      -> scientific and pipeline modules
          -> artifact/checksum/provenance primitives
```

Executable adapters should parse arguments, initialize logging, construct typed
configuration, call one reusable service, and map the result to a stable exit
status. Scientific algorithms, persisted state machines, and reusable I/O belong
in library modules.

The [module reference](../../docs/developer-guide/module-reference.md) documents
every crate-level module and its ownership.

## Command maturity

| Status | Meaning |
| --- | --- |
| `supported` | Durable maintainer capability with a stable operational contract |
| `experimental` | Useful capability whose scientific or operational contract is not production approved |
| `migration-only` | Transitional command retained for frozen evidence or by a current orchestrator; not a new-workflow entry point |
| `test-only` | Compiled helper used only by automated tests |

Candidate generation and production admission are separate. A successful command
must not be described as production approval unless every required provenance,
validation, reconciliation, checksum, and maturity gate has passed.

## Running tools

From the workspace root:

```bash
cargo run --locked --release -p nsb-data-tools --bin <command> -- --help
```

Write generated catalogues, maps, checkpoints, reports, and diagnostics to a
caller-selected external directory. Do not commit ad hoc machine output or
machine-specific absolute paths.

Verify the repository asset set with:

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

## Development rules

1. Prefer capability-oriented names over issue, phase, or machine-specific names.
2. Keep reusable implementation below `src/bin`.
3. Use typed scientific quantities and versioned schemas.
4. Persist checksums, source identity, software identity, and exact accounting.
5. Reject incompatible or unknown production state rather than guessing.
6. Resume only from verified checkpoints with documented recovery semantics.
7. Update `Cargo.toml`, `tool-registry.toml`, the tool reference, and tests in the
   same change whenever a command is added, changed, or removed.

## Validation

Relevant contract checks include:

```bash
cargo test --locked -p nsb-data-tools --test tool_registry
cargo test --locked -p nsb-data-tools --test data_product_architecture_contract
cargo test --locked -p nsb-data-tools --test deduplication_contract
cargo test --locked -p nsb-data-tools --test pipeline_contract
cargo test --locked -p nsb-data-tools --test pipeline_recovery_contract
```

Run the workspace quality gates described in the
[maintainer guide](../../docs/maintainer-guide/README.md) before proposing a data
or tooling change.