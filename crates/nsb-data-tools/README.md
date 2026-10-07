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

Moonlight scattering is split into deterministic wavelength partitions for
local reproducibility and resumability. The complete production run is
lightweight enough not to require a cluster; see the
[multiple-scattering validation report](../../docs/nsb_components/moonlight/multiple-scattering-validation.md).

The Starlight lifecycle builds receipt-backed partition shards directly at the
configured `canonical_nside`, then emits exactly one canonical map plus
`merge_report.json`. The generic checked-in `starlight-production.toml` remains
a measured-only 336–650 nm configuration and deliberately records its missing
UV/selection inputs rather than inventing them.

Candidate generation never silently registers Starlight as runtime production
data. A future bundled product requires explicit scientific, provenance, and
redistribution approval in its release change.
