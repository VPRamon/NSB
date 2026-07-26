# Starlight dataset generation

Starlight is maintained through the same dataset lifecycle as every NSB asset:

```bash
nsb-data dataset starlight update --config starlight.toml
nsb-data dataset starlight build --config starlight.toml
nsb-data dataset starlight validate --config starlight.toml
nsb-data dataset starlight publish --config starlight.toml
```

Inputs are explicit checksum-pinned sources. Large source sets assign a stable
`partition` to every source and may select the Slurm executor. Local and Slurm
workers enter the same Rust implementation and produce isolated manifests.
Partition results are admitted only after deterministic reconciliation and
dataset validation.

The bundled manual map remains an experimental reproducibility snapshot.
Publishing identical bytes does not promote its scientific maturity. A
production Gaia-derived replacement must additionally satisfy the
[science requirements](science-requirements.md), [validation
contract](map-validation.md), redistribution policy and [runtime manifest
contract](external-manifest.md).

Operational configuration, recovery and publication are documented in the
[dataset maintainer guide](../../maintainer-guide/datasets.md).
