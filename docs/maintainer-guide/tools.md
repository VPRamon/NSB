# Data-tool reference

Status: Current command reference.
Audience: Release maintainers, scientific-data maintainers, researchers, and external verifiers.
Scope: Purpose, maturity, inputs, outputs, resume behaviour, and failure semantics of every compiled `nsb-data-tools` command.

The normative machine-readable inventory is
[`crates/nsb-data-tools/tool-registry.toml`](../../crates/nsb-data-tools/tool-registry.toml).
Every `[[bin]]` entry in `crates/nsb-data-tools/Cargo.toml` must appear exactly
once in that registry and on this page.

Run any command from the workspace root:

```bash
cargo run --locked --release -p nsb-data-tools --bin <command> -- --help
```

Generated products belong in a caller-selected output directory. Production
commands fail closed on missing provenance, incomplete validation, checksum
mismatch, schema errors, or unreconciled counts.

## Status definitions

| Status | Meaning |
| --- | --- |
| Supported | Durable maintainer capability with a stable operational contract |
| Experimental | Useful capability, but its scientific or operational contract is not production approved |
| Migration-only | Transitional command retained for frozen evidence or a current orchestrator; do not base a new workflow on it |

## Command index

| Command | Area | Status | Audience |
| --- | --- | --- | --- |
| `verify_assets` | Asset verification | Supported | External users and release maintainers |
| `pack_starlight_asset` | Asset packaging | Supported | Release maintainers |
| `prepare_gaia_starlight_catalogue` | Catalogue preparation | Supported | Release maintainers |
| `prepare_tycho_starlight_catalogue` | Catalogue preparation | Experimental | Researchers |
| `query_gaia_tap` | Gaia acquisition | Supported | Release maintainers |
| `generate_gaia_starlight_release_inputs` | Gaia acquisition | Supported | Release maintainers |
| `download_gaia_xp_continuous_bulk` | Gaia acquisition | Supported | Release maintainers |
| `index_gaia_xp_continuous_bulk` | Gaia indexing | Supported | Release maintainers |
| `normalize_xp_continuous_coefficients` | XP continuous | Supported | Release maintainers |
| `reconstruct_canonical_coefficients` | XP continuous | Supported | Release maintainers |
| `validate_xp_continuous_reconstruction` | XP continuous | Supported | Release maintainers |
| `run_starlight_xp_continuous_bulk_pipeline` | XP continuous bulk orchestration | Experimental | Release maintainers |
| `generate_starlight_sample_queries` | Sampling | Supported | Release maintainers |
| `consolidate_gaia_starlight_samples` | Sampling | Supported | Release maintainers |
| `train_starlight_photometry_models` | Model development | Experimental | Researchers |
| `build_starlight_map` | Map generation | Supported | Release maintainers |
| `sweep_starlight_nside` | Map assessment | Supported | Release maintainers |
| `validate_starlight_map` | Map validation | Supported | Release maintainers |
| `audit_gaia_starlight_exclusions` | Scientific audit | Supported | Release maintainers |
| `export_starlight_healpix_to_contributions` | Integrated-product bridge | Experimental | Researchers and release maintainers |
| `build_integrated_starlight_product` | Integrated product | Supported | Release maintainers |
| `finalize_phase5_holdout_v1` | Frozen Phase 5 evidence | Migration-only | Maintainers |
| `run_phase5b_chunk_benchmark` | Transitional performance evidence | Migration-only | Maintainers |
| `run_phase5b_mini_pilot` | Transitional bulk processing | Migration-only | Maintainers |

## Asset verification and release

### `verify_assets`

**Purpose:** verify manifest coverage, file schemas, required metadata, and
checksums for the runtime asset set.

**Inputs:** `crates/nsb/data/manifest.toml` or a compatible manifest and all
referenced files.

**Outputs:** verification diagnostics and process status. It is read-only and
deterministic.

**Failure contract:** non-zero for missing or unregistered files, checksum
mismatch, invalid schemas, incomplete metadata, or manifest/header disagreement.

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

### `pack_starlight_asset`

**Purpose:** package an already validated starlight candidate as the exact runtime
map and manifest pair proposed for review or release.

**Inputs:** validated map, validation evidence, catalogue/source provenance,
checksums, and requested maturity.

**Outputs:** runtime HEALPix asset and versioned manifest. Packaging is
deterministic and has no implicit resume state.

**Failure contract:** production packaging fails on incomplete provenance,
missing independent evidence, inconsistent checksums, invalid headers, or an
unsupported maturity claim.

## Catalogue preparation

### `prepare_gaia_starlight_catalogue`

**Purpose:** convert official Gaia DR3 XP sampled bulk data, or a controlled
normalized fallback, into canonical passband-integrated starlight rows.

**Inputs:** bulk inventory or normalized input, catalogue identity, release,
license policy, photometry model, band limits, and output paths.

**Outputs:** canonical source catalogue, deterministic exclusions sidecar, and
versioned diagnostics.

**Resume:** streaming conversion; `--exclusions-only` may regenerate exclusions
evidence without replacing a validated catalogue.

**Failure contract:** non-zero for malformed spectra, invalid photometry, missing
provenance, inconsistent source accounting, checksum failure, or production-policy
violations.

### `prepare_tycho_starlight_catalogue`

**Purpose:** convert local Tycho BT/VT rows into the canonical source schema for
controlled comparison and model-development work.

**Inputs/outputs:** local catalogue plus checksum and metadata; canonical rows and
diagnostics.

**Status boundary:** experimental. Successful conversion does not establish a
production photometric calibration.

**Failure contract:** non-zero on checksum, schema, coordinate, or photometry
failure. There is no resume state.

## Gaia acquisition and indexing

### `query_gaia_tap`

**Purpose:** execute a reproducible Gaia TAP request with persisted request,
service, retry, completion, and result evidence.

**Inputs:** ADQL, endpoint policy, output paths, retry configuration, and optional
persisted job manifest.

**Outputs:** TAP result, request/status evidence, diagnostics, and a versioned job
manifest.

**Resume:** continues the same asynchronous job or reuses a persisted result only
after validation.

**Failure contract:** non-zero for terminal service errors, invalid result schema,
corrupted persisted state, or exhausted retries.

### `generate_gaia_starlight_release_inputs`

**Purpose:** produce the Gaia metadata and normalized-input bundle required by
downstream starlight processing.

**Inputs:** query policy, output directory, retrieval mode, magnitude/band limits,
license policy, validation reference, and candidate/production mode.

**Outputs:** ADQL, metadata, normalized chunks, checksums, diagnostics, and
release-input configuration.

**Resume:** `--resume` preserves only verified downloads and chunks.

**Failure contract:** production mode fails on missing XP products, parse errors,
missing provenance, or unreconciled counts.

### `download_gaia_xp_continuous_bulk`

**Purpose:** download official Gaia DR3 XP continuous partitions from a pinned
inventory.

**Inputs/outputs:** inventory, destination, storage and retry policy; verified
partitions and acquisition manifest.

**Resume:** reuses only checksum-verified complete partitions.

**Failure contract:** non-zero for missing inventory entries, corruption,
checksum mismatch, storage-policy failure, or exhausted retries.

### `index_gaia_xp_continuous_bulk`

**Purpose:** build deterministic source-to-partition lookup and planning metadata
from verified official bulk files.

**Outputs:** versioned partition/source index and diagnostics.

**Resume:** verified index partitions may be reused; rebuilding is deterministic.

**Failure contract:** non-zero when partitions, source counts, or checksums do not
reconcile with the inventory.

## Gaia XP continuous processing

### `normalize_xp_continuous_coefficients`

**Purpose:** convert raw official-bulk or DataLink coefficient records into the
strict canonical Rust schema.

**Inputs:** coefficient values, errors/correlations, source identity, and
provenance.

**Outputs:** canonical records plus a manifest with exact accepted/rejected
accounting.

**Resume:** deterministic; reuse is a caller decision after manifest/checksum
verification.

**Failure contract:** non-zero for invalid dimensions, malformed packed
correlations, missing provenance, checksum mismatch, or inconsistent accounting.

### `reconstruct_canonical_coefficients`

**Purpose:** reconstruct normalized 336–650 nm spectra from canonical coefficient
CSVs with the in-process Rust calibrator, integrate photon flux, and propagate
uncertainty.

**Inputs:** one canonical coefficient file or a directory, calibration/design
fixture, output directory, and manifest path.

**Outputs:** normalized per-source spectra unless `--integrate-only` is selected,
plus a versioned reconstruction manifest containing flux, uncertainty, and
checksums.

**Resume:** existing per-source spectra are skipped when present; the caller must
still verify manifest completeness and checksums.

**Failure contract:** non-zero when any selected record cannot be parsed,
calibrated, integrated, written, or represented in the manifest.

### `validate_xp_continuous_reconstruction`

**Purpose:** validate reconstructed spectra, integrated photon flux, uncertainty,
and calibration provenance against the frozen contract.

**Inputs/outputs:** canonical coefficients, reconstructed results, fixtures and
tolerances; versioned parity/validation report.

**Resume:** not applicable; validation is read-only and deterministic.

**Failure contract:** non-zero when spectral, integrated-flux, uncertainty, or
provenance tolerances fail.

### `run_starlight_xp_continuous_bulk_pipeline`

**Purpose:** orchestrate storage preflight, official inventory checks, rehearsal,
resume validation, partition processing, reconciliation, deterministic HEALPix
merge, and controlled cache cleanup.

**Inputs:** work/checkpoint/output/manifest directories, official checksum
inventory, optional removable-storage cache, frozen policy, calibration fixture,
limits, worker count, and cleanup policy.

**Outputs:** storage plan, session manifest, rehearsal/production metrics,
partition reconciliation, merge reports, and explicit blockers/readiness status.

**Resume:** reuses only verified cache entries, checkpoints, ledgers, and
reconciliation state.

**Status boundary:** experimental. It is the current operational bulk orchestrator,
but still depends on transitional pilot/process-launch boundaries. Do not infer
production scientific approval from a successful orchestration run.

**Failure contract:** non-zero for failed preflight gates, invalid inventory,
checkpoint/reconciliation inconsistency, processing failure, or unsafe cleanup.

## Sampling and model development

### `generate_starlight_sample_queries`

**Purpose:** generate the deterministic stratified Gaia ADQL query set defined by
the versioned sampling contract.

**Outputs:** query files and sampling manifest. Regeneration is deterministic and
has no resume state.

**Failure contract:** non-zero when a required stratum is absent or the query set
and manifest disagree.

### `consolidate_gaia_starlight_samples`

**Purpose:** validate completed sample-query jobs, deduplicate sources, and apply
the frozen spatial split.

**Inputs/outputs:** persisted TAP results/manifests and split policy; canonical
samples, inventory, exclusions, and split diagnostics.

**Resume:** reuses verified completed TAP results and recomputes consolidation
deterministically.

**Failure contract:** non-zero for missing strata, duplicate sources,
unreconciled counts, invalid job results, or split-policy violations.

### `train_starlight_photometry_models`

**Purpose:** train and compare candidate transformations on frozen train,
validation, and test splits.

**Outputs:** candidate coefficients, diagnostics, and holdout metrics.

**Status boundary:** experimental. A passing run does not constitute independent
validation or production admission.

**Failure contract:** non-zero for invalid datasets or failed requested model
checks. There is no implicit resume state.

## Starlight generation and validation

### `build_starlight_map`

**Purpose:** generate one deterministic full-sky HEALPix map from a canonical
source catalogue.

**Inputs:** catalogue, HEALPix resolution/ordering, provenance, checksums, and
diagnostics policy.

**Outputs:** HEALPix CSV and optional versioned diagnostics.

**Resume:** none; regenerate to a new path or replace intentionally.

**Failure contract:** non-zero for malformed input, invalid indices, inconsistent
flux/source accounting, required-diagnostic failure, or I/O error.

### `sweep_starlight_nside`

**Purpose:** build or reassess candidates at several HEALPix resolutions.

**Outputs:** per-resolution maps/diagnostics and a sweep summary.

**Resume:** `--assess-existing` evaluates persisted artifacts without rebuilding.

**Failure contract:** non-zero when an explicitly required candidate or production
condition is unmet. Candidate recommendation is not production admission.

### `validate_starlight_map`

**Purpose:** produce structural, scientific, and independent-reference evidence
for a generated map.

**Outputs:** versioned report with passes, failures, not-run gates, and blockers.

**Resume:** not applicable; read-only and deterministic.

**Failure contract:** non-zero for invalid maps, missing evidence, failed required
gates, or incomplete production validation.

### `audit_gaia_starlight_exclusions`

**Purpose:** reconcile every scientific exclusion with the canonical Gaia source
inventory and rejection policy.

**Outputs:** versioned exclusion audit and exact accounting evidence.

**Failure contract:** non-zero unless every exclusion is unique, justified, and
reconciled. It is read-only and deterministic.

### `export_starlight_healpix_to_contributions`

**Purpose:** convert each qualifying runtime-map pixel into a normalized
contribution row and produce an input manifest for
`build_integrated_starlight_product`.

**Inputs:** runtime map, nside, branch label, uncertainty assumptions, optional
build diagnostics, and output paths.

**Outputs:** contribution CSV, manifest, and optional coverage metadata.

**Status boundary:** experimental. The bridge records candidate limitations and
does not prove full 300–650 nm coverage or production readiness.

**Failure contract:** non-zero on invalid map columns/indices/radiance, invalid
uncertainty parameters, or output/checksum failure. No implicit resume.

### `build_integrated_starlight_product`

**Purpose:** combine approved population contributions into an integrated
candidate and evaluate remaining production blockers.

**Inputs:** contribution files/manifests, frozen policies, validation references,
provenance, and output configuration.

**Outputs:** integrated candidate, diagnostics, approval evidence, and release
metadata.

**Resume:** no hidden state; persisted inputs are supplied explicitly.

**Failure contract:** success applies only to the requested candidate or production
condition; production admission fails closed while any gate remains unresolved.

## Transitional commands

These commands are compiled because frozen evidence or the current experimental
bulk orchestrator still depends on them. They are not supported entry points for
a new data release.

| Command | Current role | Replacement direction |
| --- | --- | --- |
| `finalize_phase5_holdout_v1` | Reproduce the frozen holdout-v1 download, normalization, Rust reconstruction, independence, and preflight evidence | Capability-oriented validators and the shared pipeline framework |
| `run_phase5b_mini_pilot` | Bounded streaming, checkpoint, Rust calibration, and HEALPix implementation used by the experimental bulk path | Library/service-owned partition processor without phase naming or sibling-process launch |
| `run_phase5b_chunk_benchmark` | Measure mini-pilot batch-size throughput and RSS | Criterion or dedicated capability benchmark calling library services directly |

Do not copy their machine-specific defaults or phase terminology into current
user instructions. Reusable behaviour belongs in library modules; immutable
historical evidence belongs in fixtures and clearly labelled reports.

## Adding, changing, or removing a command

A command change is incomplete unless it updates all of the following:

1. `crates/nsb-data-tools/Cargo.toml`;
2. `crates/nsb-data-tools/tool-registry.toml`;
3. this reference;
4. the relevant data-update or scientific workflow;
5. tests for registry completeness, output contracts, and recovery semantics.

A new executable must represent a durable capability, remain a thin adapter over
library/service code, and define versioned outputs, resume/idempotency, and stable
failure behaviour.