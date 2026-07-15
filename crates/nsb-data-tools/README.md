# nsb-data-tools

`nsb-data-tools` is the private Rust crate that acquires, constructs, validates,
and packages NSB scientific data products. Runtime NSB never invokes it.

## Start here

The only supported executable is `nsb-data`:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- --help
cargo run --locked -p nsb-data-tools --bin nsb-data -- starlight --help
```

Commands are grouped by the starlight workflow: acquisition, catalogue
preparation, XP-continuous reconstruction, sampling, map construction, quality,
product assembly, and release. The normative action inventory and source of the
generated human reference is [tool-registry.toml](tool-registry.toml). Read the
[maintainer tool reference](../../docs/maintainer-guide/tools.md) to choose an
action and understand its inputs, outputs, resume semantics, and failure mode.

## Maintenance policy

Each action is a durable, reusable capability with one owning service and a
typed contract. Common persistence, checksums, provenance, logging, and pipeline
logic live in shared modules rather than action implementations. A new action
must be registered before it is exposed, and the generated reference must be
current:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  maintenance render-tool-reference --check
```

Use `--write` only when intentionally regenerating the checked-in reference.
Remove obsolete actions completely: command route, service, tests, registry
entry, and documentation. Legacy aliases, phase/pilot code, shell wrappers, and
ad-hoc scripts are not retained.
