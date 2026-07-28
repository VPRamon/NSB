# Starlight dataset generation

Starlight is maintained through the same dataset lifecycle as every NSB asset:

```bash
nsb-data dataset starlight update --config starlight.toml
nsb-data dataset starlight build --config starlight.toml
nsb-data dataset starlight validate --config starlight.toml
nsb-data dataset starlight publish --config starlight.toml
```

The production configuration imports the official GaiaSource and XP continuous
checksum inventories. Both products must expose exactly the same source-range
partitions. Large acquisition and build runs use those reconciled ranges as
Slurm array tasks; local and Slurm workers enter the same Rust implementation
and produce isolated manifests. Partition results are admitted only after
checksum verification, exact accounting, deterministic reconciliation and
dataset validation.

Raw downloads are resumable and enter a content-addressed SHA-256 cache only
after their official checksum passes. HEALPix partition checkpoints are sparse
and merged in canonical partition order, so scheduler completion order cannot
change the final bytes.

## Resolution and quantity contract

`flux_ph_m2_s` is integrated photon flux per HEALPix pixel, in
`ph m-2 s-1`; it is not surface radiance. The nside-128 map is the canonical
Gaia source accumulation. The nside-64 map is a conservative NESTED
downsample. The nside-256 and nside-512 maps are diagnostic conservative
upsamples and contain no new spatial information.

For an order increase of `order_delta`, each parent has
`1 << (2 * order_delta)` children. The generator divides parent flux uniformly
by that count, corresponding to equal child areas. Integer admitted/excluded
source counts cannot be divided continuously, so the quotient is assigned to
every child and the remainder to the lowest child indices. This deterministic
apportionment preserves accounting exactly but does not represent physical
source localization.

The checked-in resolution sweep can be repaired without reprocessing Gaia:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  _repair-starlight-resolution-sweep --data-dir crates/nsb/data
```

This command reads the canonical nside-128 payload, preserves nside-64 values,
regenerates nside 256/512, writes the versioned headers, updates report
summaries/checksums, and validates the complete parent-child sweep.

The bundled manual map remains an experimental reproducibility snapshot.
Publishing identical bytes does not promote its scientific maturity. A
production Gaia-derived replacement must additionally satisfy the
[science requirements](science-requirements.md), [validation
contract](map-validation.md), redistribution policy and [runtime manifest
contract](external-manifest.md).

Operational configuration, recovery and publication are documented in the
[dataset maintainer guide](../../maintainer-guide/datasets.md).

The evidence retained for the maps already present in `crates/nsb/data`,
including known limitations in their historical reproduction record, is
documented in [Provenance of existing starlight datasets](existing-datasets.md).

## Production hardening notes (2026-07-28)

During the full Gaia DR3 production run, one XP partition contained an invalid
`bp_n_parameters=null` row in the upstream bulk ECSV. Strict integer parsing
caused one partition worker to fail even though the row could not be calibrated.

The XP bulk stream now skips rows that fail canonical parsing. This behavior is
consistent with existing fail-closed handling in the worker path: records that
cannot be calibrated are excluded from admitted flux and are tracked through
partition/source accounting gates during `validate`.

Operationally, if a single partition fails in Slurm while the rest complete,
rerun only the missing partition with `--partitions <id>` and then rerun
`validate` before `publish`.
