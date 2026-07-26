# XP continuous production tools

Operational entry points are exposed only through the hierarchical Rust CLI:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  starlight acquire xp-bulk download --help

cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  starlight xp-continuous process-partition --help

cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  starlight xp-continuous run-bulk --help

cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  starlight product export-contributions --help
```

Python generators and shell orchestration wrappers are intentionally not part of the supported data-product surface.
