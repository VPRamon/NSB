# Starlight dataset generation

Starlight uses the common dataset lifecycle:

```bash
nsb-data dataset starlight update --config starlight.toml
nsb-data dataset starlight build --config starlight.toml
nsb-data dataset starlight validate --config starlight.toml
nsb-data dataset starlight publish --config starlight.toml
```

The production configuration imports the official GaiaSource and XP continuous
checksum inventories. Both products must expose the same source-range
partitions. Downloads enter the content-addressed cache only after checksum
verification. Local and Slurm workers use the same Rust implementation and
write isolated, strictly validated partition shards.

## One canonical map

Each Starlight dataset version has exactly one `canonical_nside`:

```toml
[starlight.map]
canonical_nside = 128
```

Every Gaia source contribution is accumulated directly into that resolution.
The reconciled shards produce:

```text
starlight_nside{canonical_nside}.csv
merge_report.json
```

The current candidate is nside 128. Changing `canonical_nside` changes the
configuration checksum and run identity and requires a clean source-level
generation, fresh report, validation, provenance, and scientific review. A
higher-resolution release must never use a lower-resolution map as its input.

`flux_ph_m2_s` is integrated photon flux per HEALPix pixel in `ph m-2 s-1`.
Runtime queries may convert a pixel-integrated quantity into the runtime
radiance contract using pixel solid angle; that does not make the candidate CSV
a surface-radiance field.

Resolution selection, when needed, is a separate scientific study comparing
independent source-level runs. Only the selected candidate is published.
Diagnostic resampling is outside the scientific publication lifecycle.

The bundled manual seed remains a separate experimental snapshot. Publishing
identical bytes does not promote its scientific maturity. A production
Gaia-derived replacement must additionally satisfy the
[science requirements](science-requirements.md), [validation
contract](map-validation.md), redistribution policy, and [runtime manifest
contract](external-manifest.md).

Operational recovery and publication are documented in the
[dataset maintainer guide](../../maintainer-guide/datasets.md). Historical
artifacts and limitations are recorded in
[Provenance of existing starlight datasets](existing-datasets.md).

## Production hardening note

The full Gaia DR3 run encountered an upstream XP row with
`bp_n_parameters=null`. Canonical parsing excludes records that cannot be
calibrated and retains exact partition/source accounting. If a Slurm partition
fails, rerun only that partition and then repeat validation before publication.
