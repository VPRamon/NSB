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

The first-release candidate schema is `nsb-healpix-starlight-candidate-v1`.
It is sparse, strictly pixel-sorted, uses Galactic NESTED HEALPix indexing,
stores integrated photon flux per pixel, and defines omitted pixels as zero
flux and zero source counts. The combined 300–650 nm product uses this same
contract; the pre-release reset preserves all scientific values and input/model
identities while regenerating checksums.

Changing the map resolution, scientific policy, calibration artifact,
source-selection contract, or input checksums creates a new candidate and
requires fresh validation and admission evidence.

## Runtime packing

A checksum-pinned `candidate-v1` is packed to RING with the
`starlight-runtime-pack-v1` packer. Deterministic outputs use
`nsb-healpix-starlight-v1` and `nsb-starlight-runtime-manifest-v1` with
`schema_version = 1`. The lossless compact representation uses
`nsb-healpix-starlight-binary-v1` with the same v1 provenance contract.
See the [reset record](release-candidate/schema-reset-v1.md) for the checksum
migration and the exact renewed conditional authorization.

New candidate products remain outside the release registry unless they pass
independent scientific validation, redistribution approval, and production
admission. See the [release-candidate lifecycle](release-candidate/README.md).

Operational recovery and cluster execution are documented in the
[dataset maintainer guide](../../maintainer-guide/datasets.md).
