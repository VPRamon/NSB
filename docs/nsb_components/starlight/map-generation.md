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

The generic first-public-release schema for **new candidate runs** is
`nsb-healpix-starlight-candidate-v1`. It is sparse, strictly pixel-sorted,
uses Galactic NESTED HEALPix indexing, stores integrated photon flux per pixel,
and defines omitted pixels as zero flux and zero source counts.

The distinct, already-frozen 300–650 nm Starlight candidate retains
`nsb-healpix-starlight-candidate-v5`: its exact bytes, checksums and scientific
validation evidence predate this public-baseline reset. It must not be relabelled
as `v1` without regenerating and reapproving the product.

Changing the map resolution, scientific policy, calibration artifact,
source-selection contract, or input checksums creates a new candidate and
requires fresh validation and admission evidence.

## Runtime packing

An admitted generic `candidate-v1` can be packed to RING with the
`starlight-runtime-pack-v1` packer. Packing is deterministic and preserves the
candidate checksum in provenance. Its baseline outputs are
`nsb-healpix-starlight-v1` and `nsb-starlight-runtime-manifest-v1`.

The approved frozen `candidate-v5` follows its separately validated path:
`nsb-healpix-starlight-v2` (packed runtime CSV) and
`nsb-starlight-runtime-manifest-v2` (provenance sidecar). The production package
embeds the approved compact `nsb-healpix-starlight-binary-v1` representation
with its corresponding v2 provenance, not the original candidate/CSV/report.
This existing scientific evidence is checksum-pinned and must not be relabelled
for a cosmetic first-release reset.

New candidate products remain outside the release registry unless they pass
independent scientific validation, redistribution approval, and production
admission. See the [release-candidate lifecycle](release-candidate/README.md)
and [schema versioning policy](../../specifications/schema-versioning.md).

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
