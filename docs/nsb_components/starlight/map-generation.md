# Starlight dataset generation

Starlight uses the common dataset lifecycle:

```bash
nsb-data dataset starlight update --config crates/nsb-data-tools/config/starlight-production.toml
nsb-data dataset starlight build --config crates/nsb-data-tools/config/starlight-production.toml
nsb-data dataset starlight validate --config crates/nsb-data-tools/config/starlight-production.toml
nsb-data dataset starlight publish --config crates/nsb-data-tools/config/starlight-production.toml
```

The production configuration imports official GaiaSource and XP continuous
checksum inventories. Downloads enter the content-addressed cache only after
checksum verification. Local and Slurm workers use the same Rust
implementation and write isolated, validated partition shards.

Bright-star supplementation is built offline from explicitly supplied
catalogue/reference inputs. Raw upstream catalogue bytes remain external. A
candidate's merge report records exact input identities, checksums, source
accounting, calibration models, and uncertainty policy.

## Canonical map

Each run selects one `canonical_nside`:

```toml
[starlight.map]
canonical_nside = 128
```

Workers accumulate directly into that resolution. Reconciliation produces:

```text
starlight_nside{canonical_nside}.csv
merge_report.json
```

The first public candidate schema is
`nsb-healpix-starlight-candidate-v1`. It is sparse, strictly pixel-sorted,
uses Galactic NESTED HEALPix indexing, stores integrated photon flux per pixel,
and defines omitted pixels as zero flux and zero source counts.

Changing the map resolution, scientific policy, calibration artifact,
source-selection contract, or input checksums creates a new candidate and
requires fresh validation and admission evidence.

## Runtime packing

An admitted candidate can be packed to the runtime RING representation with
the `starlight-runtime-pack-v1` packer. Packing is deterministic and preserves
the candidate checksum in provenance. Promotion emits the first public runtime
schemas:

- `nsb-healpix-starlight-v1`
- `nsb-starlight-runtime-manifest-v1`

Generated candidates and packed runtime assets remain outside the NSB 0.1.0
release registry until redistribution and production admission are explicitly
approved.

## Inspection

A generated map can be visualized with the Rust example by supplying its path
explicitly:

```bash
cargo run --release --locked -p nsb --example starlight_heatmap -- \
  --map /path/to/starlight_nside128.csv \
  --output starlight_heatmap.png
```

The example validates the map contract before plotting.

Operational recovery and cluster execution are documented in the
[dataset maintainer guide](../../maintainer-guide/datasets.md).
