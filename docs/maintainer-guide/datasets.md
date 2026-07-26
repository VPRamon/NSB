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

## Slurm starlight execution

Set `execution.executor = "slurm"` and provide `execution.slurm`. Each source
must carry a stable `partition` identifier. The orchestrator submits an
`sbatch --array` invocation of the internal worker in the same Rust binary;
there are no maintained shell or Python wrappers.

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

All workers verify their configured partition inputs and write isolated,
checksum-pinned state. A run may be resumed with its manifest without changing
the selected partition set.
