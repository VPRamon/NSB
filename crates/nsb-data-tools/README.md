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

The production Starlight lifecycle builds receipt-backed nside-128 partition
shards, then emits candidate maps at nside 64, 128, 256, and 512 plus
`merge_report.json`. The report explicitly identifies the current
join-only/identity-selection policy and missing 300–336 nm correction; these
candidate artifacts are not silently registered as runtime production data.
