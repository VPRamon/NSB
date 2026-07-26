# Reproducible dataset maintenance

`nsb-data` exposes four datasets and four lifecycle operations:

| Dataset | Published artifacts | Execution |
| --- | --- | --- |
| `airglow-continuum` | `airglow_cont.dat` | local |
| `solar-spectrum` | `solar_spectrum.dat` | local |
| `moonlight-scattering` | `mie_m15s1.dat`, `sscatcor_m15s1.dat` | local |
| `starlight` | validated HEALPix map artifacts | local or Slurm |

The supported lifecycle is `update → build → validate → publish`. `update`
verifies every source checksum before copying it into the run workspace.
`build` produces deterministic runtime artifacts, `validate` records
machine-readable gates, and `publish` refuses changed or unvalidated outputs.
Publishing updates `crates/nsb/data` and its manifest but never commits.

## Configuration

Every command requires a versioned TOML file. Relative paths are resolved
against that file, never the current directory. The repository configurations
under `crates/nsb-data-tools/config/` reproduce the currently bundled assets.
Each source defines exactly one local `path` or HTTPS `url` plus its mandatory
SHA-256; downloaded bytes are never admitted before verification.

```toml
schema_version = 1
dataset = "solar-spectrum"

[workspace]
root = "/shared/nsb/runs/solar-spectrum"

[execution]
executor = "local"
concurrency = 1

[[sources]]
name = "solar_spectrum.dat"
path = "/shared/nsb/sources/solar_spectrum.dat"
sha256 = "64-lowercase-hex-characters"

[publish]
repository_root = "/checkout/nsb"
```

The historical airglow, solar and scattering snapshots remain limited by their
incomplete upstream provenance and licensing. Reproducibility of their current
bytes does not promote their scientific maturity. Replacing one requires a
reviewed source, license, checksum, validation evidence and manifest update.

## Local operation and recovery

```bash
nsb-data dataset solar-spectrum update --config run.toml
nsb-data dataset solar-spectrum build --config run.toml
nsb-data dataset solar-spectrum validate --config run.toml
nsb-data dataset solar-spectrum publish --config run.toml
nsb-data run status --run /shared/nsb/runs/solar-spectrum/runs/solar-spectrum/build/run.json
nsb-data run resume --run /shared/nsb/runs/solar-spectrum/runs/solar-spectrum/build/run.json
```

Run manifests pin the resolved workspace, configuration checksum, Git commit,
executor, partitions, artifacts and validation report. Atomic output promotion
prevents a partial file from being treated as complete.

## Full Starlight source acquisition

Full Starlight uses
`crates/nsb-data-tools/config/starlight-production.toml`. Its partitions come
from the reconciled official GaiaSource and XP continuous checksum inventories;
they are not hand-written into TOML.

First create or refresh the two normalized inventories on a login node:

```bash
nsb-data dataset starlight update \
  --config crates/nsb-data-tools/config/starlight-production.toml
```

Then acquire every paired partition through Slurm:

```toml
[execution]
executor = "slurm"
concurrency = 8

[execution.slurm]
partition = "compute"
account = "nsb"
time_limit = "12:00:00"
memory = "8G"
array_parallelism = 32
```

```bash
nsb-data dataset starlight update \
  --config crates/nsb-data-tools/config/starlight-production.toml \
  --executor slurm
```

The orchestrator submits one `sbatch --array` task per reconciled range. Every
task enters the internal worker in the same Rust binary, resumes an interrupted
HTTP transfer, verifies the official checksum, computes SHA-256, and promotes
the bytes to `cache/objects/sha256/<digest>`. A strict receipt under
`cache/receipts/<product>/<partition>.json` binds the URL, official checksum,
SHA-256, size, and object path. No maintained shell or Python wrapper exists.

For a bounded local acquisition, select explicit ranges:

```bash
nsb-data dataset starlight update \
  --config crates/nsb-data-tools/config/starlight-production.toml \
  --partitions 000000-003111,003112-005263
```

Use the submitted run manifest printed by the command:

```bash
nsb-data run status --run /shared/nsb/starlight/runs/starlight/<run-id>/run.json
nsb-data run resume --run /shared/nsb/starlight/runs/starlight/<run-id>/run.json
```

Resume submits only partitions without a complete, checksum-valid worker
manifest. A changed TOML, partition set, software revision, inventory contract,
or cached object cannot silently enter an existing run.

## Full Starlight build readiness

Inventory reconciliation, local/Slurm acquisition, durable run state, sparse
HEALPix shard accumulation, and deterministic shard merge are implemented.
Production `build` currently fails closed while XP coefficient reconstruction,
the GaiaSource/XP streaming join, population correction, and final scientific
admission gates are completed. The snapshot configuration remains independently
reproducible, but is not a production substitute.
