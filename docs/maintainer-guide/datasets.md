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
under `crates/nsb-data-tools/config/` define the supported regeneration
workflows. They do not, by themselves, prove byte-for-byte reproducibility of
historical artifacts whose original run evidence was not retained; the
starlight exceptions are recorded in
[Provenance of existing starlight datasets](../nsb_components/starlight/existing-datasets.md).
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
they are not hand-written into TOML. Before running it, set `[workspace].root`
to a shared scratch or NVMe filesystem mounted at the same absolute path on
the login and compute nodes. Never point the lifecycle workspace at USB media.
The checked-in `/shared/scratch/nsb/starlight-production` is a site-local
example.

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
time_limit = "24:00:00"
memory = "16G"
array_parallelism = 16
```

These conservative defaults bound simultaneous CDN and shared-filesystem I/O.
Override the partition/account for the site, then tune memory, time, and array
parallelism from the two-partition smoke measurements rather than increasing
them blindly.

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

Before the full arrays, run the same Rust acquisition and build path on two
explicit ranges:

```bash
nsb-data dataset starlight update \
  --config crates/nsb-data-tools/config/starlight-production.toml \
  --partitions 000000-003111,003112-005263

nsb-data dataset starlight build \
  --config crates/nsb-data-tools/config/starlight-production.toml \
  --partitions 000000-003111,003112-005263
```

The smoke succeeds when both
`workers/000000-003111/shard.json` and
`workers/003112-005263/shard.json` exist and the top-level build run manifest
printed by the local command reports `complete`. Full validation deliberately
rejects this partial shard set. The
automated Rust fixture test additionally exercises receipt verification,
GaiaSource/XP joining, calibration, shard validation, map emission, and map
format validation without network access.

Use the submitted run manifest printed by the command:

```bash
nsb-data run status --run /shared/nsb/starlight/runs/starlight/<run-id>/run.json
nsb-data run resume --run /shared/nsb/starlight/runs/starlight/<run-id>/run.json
```

Resume submits only partitions without a complete, checksum-valid worker
manifest. A changed TOML, partition set, software revision, inventory contract,
or cached object cannot silently enter an existing run.

## Full Starlight build and release

Production build workers load the pinned GaiaXPy 2.1.4 continuous-design
fixture, stream each compressed XP partition, and admit a source only when its
identifier is also present in the paired GaiaSource partition. Each worker
writes a strict sparse nside-128 shard at
`workers/<partition>/shard.json`; this is the authoritative worker artifact.
Local validation checksum-verifies these shards and copies them to the
canonical reconciliation input `outputs/shards/<partition>.json` before
merging. Compute nodes and the validating login node must therefore share the
same workspace.

The versioned admission policy `gaia-dr3-xp-continuous-join-v1` applies these
rules exactly:

- an XP row without the paired GaiaSource identifier is excluded as
  `no_gaia_source_match`;
- a record rejected by frozen calibration is `calibration_failed`;
- a non-finite or non-positive signed integral is `invalid_flux`;
- a non-finite or negative propagated statistical uncertainty is
  `invalid_uncertainty`;
- every other joined row is admitted at weight 1 with its XP statistical
  variance; no independent systematic term is invented.

The current candidate deliberately has no calibrated Gaia selection-function
or faint-tail correction. `merge_report.json` records the versioned
`selection-function-identity-stub-v1` policy with bounded weights `[1,1]`,
`applied=false`, and no residual-tail estimate. It also records that direct
spectral coverage is 336–650 nm and that no validated 300–336 nm correction is
applied. These declarations prevent the candidate from silently claiming the
fully corrected 300–650 nm production contract in
`docs/nsb_components/starlight/science-requirements.md`.

Run the full workers through the configured Slurm array:

```bash
nsb-data dataset starlight build \
  --config crates/nsb-data-tools/config/starlight-production.toml \
  --executor slurm
```

No usable legacy Gaia bulk files were available on mounted local/USB storage
when this bridge was completed, so the supported bridge is receipt-first: run
the full Slurm `update` array before using the old ledger. The ledger can then
be used as a read-only scheduling index. A legacy-completed partition is
skipped only when its XP object has a valid receipt and SHA-256-verified CAS
object in the new workspace:

```bash
nsb-data dataset starlight build \
  --config crates/nsb-data-tools/config/starlight-production.toml \
  --executor slurm \
  --skip-completed-from "$HOME/nsb-data/starlight-gaia-release/checkpoints"
```

Legacy dense nside-64 accumulator bytes are never accepted as a production
shard. The recorded 61,201,322 of 184,729,270 sources (33.13%, 710 ledger
entries) therefore prioritize the 2,676 not-yet-processed ranges; they do not
become new evidence. After that array completes, submit
the same full build once without `--skip-completed-from`. Workers already
completed under the new lifecycle return from their checksum-valid manifests,
while the 710 legacy ranges are rebuilt as nside-128 `PartitionShard` files.
Only then can validation see all 3,386 shards. The progress-guard backup and
checkpoint directory remain read-only throughout.

After all workers finish, run validation locally:

```bash
nsb-data dataset starlight validate \
  --config crates/nsb-data-tools/config/starlight-production.toml
```

Validation reconciles checksum-valid worker shards, performs a canonical merge,
and emits:

- `starlight_nside128.csv`, the candidate sparse target map
- `starlight_nside64.csv`, the conservative nested downsample
- `starlight_nside256.csv`, the diagnostic conservative upsample
- `starlight_nside512.csv`, the diagnostic conservative upsample
- `merge_report.json`, including per-resolution flux/source totals and drift,
  map checksums, exclusions,
  explicit science-policy limitations, and the deterministic partial-merge
  reference

The release gates verify artifact checksum round trips, finite flux, at least
70% occupied nside-128 pixels in the Galactic plane (`|b| < 20°`), exact
observed/admitted/excluded population accounting, and a pixel checksum stable
across an independent partial merge. The policy gate also verifies that the
identity selection stub and missing 300–336 nm correction remain explicit.
The independent `cross-resolution-flux-conservation` and
`cross-resolution-source-accounting` gates additionally require at most 0.1%
flux drift and exact integer accounting across the complete sweep. Upsampled
maps divide integrated parent-pixel flux by 4 or 16; their apportioned source
counts are deterministic bookkeeping, not source localization.
Publish accepts only unchanged artifacts from a passing validation report,
copies them into `crates/nsb/data`, and updates or creates checksum registry
entries. Newly created Starlight entries are deliberately
`calibration_status = "candidate"` and `runtime_embedded = false`; human,
independent-reference, and redistribution gates are still required for
production admission:

```bash
nsb-data dataset starlight publish \
  --config crates/nsb-data-tools/config/starlight-production.toml
```

The snapshot configuration remains independently reproducible and is not a
production substitute.

## Cluster runbook

Run these stages in order from the same checkout and shared workspace:

1. On the login node, create and pin both official inventories:
   `nsb-data dataset starlight update --config crates/nsb-data-tools/config/starlight-production.toml`.
2. Acquire and receipt every GaiaSource/XP pair:
   `nsb-data dataset starlight update --config crates/nsb-data-tools/config/starlight-production.toml --executor slurm`.
3. Optionally prioritize the non-legacy ranges with
   `nsb-data dataset starlight build --config crates/nsb-data-tools/config/starlight-production.toml --executor slurm --skip-completed-from "$HOME/nsb-data/starlight-gaia-release/checkpoints"`.
   Then run
   `nsb-data dataset starlight build --config crates/nsb-data-tools/config/starlight-production.toml --executor slurm`
   without the skip flag to backfill every range lacking a new shard.
4. After `nsb-data run status` reports the build array complete, run
   `nsb-data dataset starlight validate --config crates/nsb-data-tools/config/starlight-production.toml`
   locally on a node that can see the shared workspace.
5. Review `validation.json` and `outputs/merge_report.json`, then run
   `nsb-data dataset starlight publish --config crates/nsb-data-tools/config/starlight-production.toml`
   locally only when candidate publication is intended.
