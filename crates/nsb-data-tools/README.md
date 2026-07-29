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

The production Starlight lifecycle builds receipt-backed partition shards
directly at the configured `canonical_nside`, then emits exactly one canonical
map plus `merge_report.json`. The current Gaia-derived candidate uses nside
128. The report explicitly identifies the current
join-only/identity-selection policy and missing 300–336 nm correction; these
candidate artifacts are not silently registered as runtime production data.
The fail-closed 300–336 nm correction artifact and reproducibility command are
documented in
[`docs/maintainer-guide/starlight-uv-calibration.md`](../../docs/maintainer-guide/starlight-uv-calibration.md).
No production UV artifact is currently configured.
