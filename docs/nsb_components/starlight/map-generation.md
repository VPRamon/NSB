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

The bundled manual map remains an experimental reproducibility snapshot.
Publishing identical bytes does not promote its scientific maturity. A
production Gaia-derived replacement must additionally satisfy the
[science requirements](science-requirements.md), [validation
contract](map-validation.md), redistribution policy and [runtime manifest
contract](external-manifest.md).

Operational configuration, recovery and publication are documented in the
[dataset maintainer guide](../../maintainer-guide/datasets.md).

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
