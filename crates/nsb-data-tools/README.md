# nsb-data-tools

`nsb-data-tools` is the Rust-only maintainer crate for reproducible NSB
datasets. Its sole executable is `nsb-data`; runtime NSB never invokes it.

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset list
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  dataset solar-spectrum update --config crates/nsb-data-tools/config/solar-spectrum.toml
```


## F10.7 solar activity

Network acquisition for F10.7 belongs here (never in the `nsb` runtime):

```bash
nsb-data solar f107 update --fixture-dir crates/nsb-data-tools/tests/fixtures/swpc
nsb-data solar f107 status
nsb-data solar f107 resolve --time 2026-08-20T12:00:00Z
nsb-data solar f107 import path/to/store.json
nsb-data solar f107 verify path/to/store.json --sha256 <digest>
```

See [`docs/nsb_components/airglow/f107-resolver.md`](../../docs/nsb_components/airglow/f107-resolver.md).

The public contract, configuration reference, local/Slurm execution model and
publication workflow are documented in
[`docs/maintainer-guide/datasets.md`](../../docs/maintainer-guide/datasets.md).

The Starlight lifecycle builds receipt-backed partition shards directly at the
configured `canonical_nside`, then emits exactly one canonical map plus
`merge_report.json`. The generic checked-in `starlight-production.toml` remains
a measured-only 336–650 nm configuration and deliberately records its missing
UV/selection inputs rather than inventing them.

The frozen candidate used for the current release review is different: the
nside-128 combined 300–650 nm Ladon run pins external UV, photometric,
selection-function, and bright-star-supplement artifacts and records their
checksums/provenance in the candidate and merge report. Those large calibration
inputs are not embedded in the repository. See
[`docs/maintainer-guide/starlight-uv-calibration.md`](../../docs/maintainer-guide/starlight-uv-calibration.md)
and the [Starlight release-candidate bundle](../../docs/nsb_components/starlight/release-candidate/README.md).

Neither candidate generation path silently registers Starlight as runtime
production data; #103 redistribution approval is still required before bundled
activation.
