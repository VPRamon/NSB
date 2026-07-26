# nsb-data-tools

`nsb-data-tools` is the Rust-only maintainer crate for reproducible NSB
datasets. Its sole executable is `nsb-data`; runtime NSB never invokes it.

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset list
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  dataset solar-spectrum update --config crates/nsb-data-tools/config/solar-spectrum.toml
```

The public contract, configuration reference, local/Slurm execution model and
publication workflow are documented in
[`docs/maintainer-guide/datasets.md`](../../docs/maintainer-guide/datasets.md).
