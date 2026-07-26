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
