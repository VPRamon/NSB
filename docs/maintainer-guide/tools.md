# Data-tool reference

Status: Current human-readable reference for retained `nsb-data-tools` commands.
Audience: Release maintainers, scientific-data maintainers, researchers, and external verifiers.
Scope: Purpose, audience, inputs, outputs, resume behaviour, and failure semantics of every supported command.

The normative machine-readable inventory is
`crates/nsb-data-tools/tool-registry.toml`. This guide explains the same command
surface as a maintainer workflow. When this guide and the registry disagree, fix
both in the same change; the registry remains the CI authority.

Run a tool from the workspace root:

```bash
cargo run --locked --release -p nsb-data-tools --bin <tool> -- --help
```

Use a caller-selected output directory. Production commands are fail-closed:
missing provenance, incomplete validation, checksum mismatch, schema errors, or
unreconciled counts must produce a non-zero exit status.

## Command overview

| Command | Area | Audience | Status |
| --- | --- | --- | --- |
| `verify_assets` | Asset verification | External users and release maintainers | Supported |
| `pack_starlight_asset` | Asset packaging | Release maintainers | Supported |
| `prepare_gaia_starlight_catalogue` | Catalogue preparation | Release maintainers | Supported |
| `prepare_tycho_starlight_catalogue` | Catalogue preparation | Researchers | Experimental |
| `query_gaia_tap` | Gaia acquisition | Release maintainers | Supported |
| `generate_gaia_starlight_release_inputs` | Gaia acquisition | Release maintainers | Supported |
| `download_gaia_xp_continuous_bulk` | Gaia acquisition | Release maintainers | Supported |
| `index_gaia_xp_continuous_bulk` | Gaia acquisition | Release maintainers | Supported |
| `normalize_xp_continuous_coefficients` | Gaia XP continuous | Release maintainers | Supported |
| `validate_xp_continuous_reconstruction` | Gaia XP continuous | Release maintainers | Supported |
| `generate_starlight_sample_queries` | Sampling | Release maintainers | Supported |
| `consolidate_gaia_starlight_samples` | Sampling | Release maintainers | Supported |
| `train_starlight_photometry_models` | Model development | Researchers | Experimental |
| `build_starlight_map` | Map generation | Release maintainers | Supported |
| `sweep_starlight_nside` | Map assessment | Release maintainers | Supported |
| `validate_starlight_map` | Map validation | Release maintainers | Supported |
| `audit_gaia_starlight_exclusions` | Scientific audit | Release maintainers | Supported |
| `build_integrated_starlight_product` | Integrated product | Release maintainers | Supported |

## Asset verification and release

### `verify_assets`

**Use when:** checking a source checkout, CI build, release candidate, or
installed asset set against the runtime manifest.

**Inputs:** `crates/nsb/data/manifest.toml` or another compatible manifest and
all referenced assets.

**Outputs:** a human-readable verification result and process exit status.

**Resume/idempotency:** read-only and deterministic; resume is not applicable.

**Failure contract:** non-zero for missing files, unregistered files, checksum
mismatch, invalid schemas, incomplete metadata, or manifest/header disagreement.

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

### `pack_starlight_asset`

**Use when:** converting an already validated starlight candidate into the exact
runtime map and manifest pair proposed for review or release.

**Inputs:** validated map, validation evidence, source provenance, catalogue
checksums, and requested maturity.

**Outputs:** runtime HEALPix asset and its versioned manifest.

**Resume/idempotency:** deterministic from immutable inputs; no implicit resume.

**Failure contract:** production packaging fails on incomplete provenance,
missing independent evidence, inconsistent checksums, invalid headers, or an
unsupported maturity claim.

## Catalogue preparation

### `prepare_gaia_starlight_catalogue`

**Use when:** converting official Gaia DR3 XP sampled bulk data, or a controlled
normalized fallback, into canonical passband-integrated starlight source rows.

**Inputs:** bulk inventory or normalized input, catalogue identity, release,
license policy, photometry model, band limits, and output paths.

**Outputs:** canonical source catalogue, deterministic exclusions sidecar, and
versioned diagnostics.

**Resume/idempotency:** streaming full conversion. `--exclusions-only` may
regenerate exclusions evidence without replacing an existing validated
catalogue.

**Failure contract:** non-zero for malformed spectra, invalid photometry,
missing provenance, inconsistent source accounting, checksum failures, or
production-policy violations.

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin prepare_gaia_starlight_catalogue -- \
  --bulk-dir /data/gaia-dr3-xp-sampled \
  --output /data/starlight/gaia_sources.csv \
  --diagnostics-output /data/starlight/gaia_sources.diagnostics.json \
  --exclusions-output /data/starlight/gaia_exclusions.csv \
  --catalog-name Gaia \
  --catalog-release DR3 \
  --catalog-license "$GAIA_DERIVED_PRODUCT_LICENSE_POLICY" \
  --photometry-model gaia_dr3_xp_photon_radiance_336_650nm_v1 \
  --band-min-nm 336 \
  --band-max-nm 650
```

### `prepare_tycho_starlight_catalogue`

**Use when:** performing controlled Tycho BT/VT experiments against the
canonical starlight source schema.

**Inputs:** local Tycho-like catalogue, verified input checksum, catalogue
metadata, and conversion policy.

**Outputs:** canonical source catalogue and diagnostics.

**Resume/idempotency:** deterministic full conversion; no resume.

**Failure contract:** non-zero on checksum, schema, coordinate, or photometry
failure. Output remains experimental and must not be presented as a production
starlight calibration.

## Gaia acquisition and indexing

### `query_gaia_tap`

**Use when:** executing a reproducible Gaia TAP query with persisted service and
result evidence.

**Inputs:** ADQL, TAP endpoint policy, output paths, retry configuration, and
optional persisted job manifest.

**Outputs:** TAP result, request/status evidence, diagnostics, and a versioned
job manifest.

**Resume/idempotency:** explicit resume continues the same asynchronous job or
reuses a persisted result only after validation.

**Failure contract:** non-zero for terminal service errors, invalid result
schema, corrupted persisted state, or exhausted retries.

### `generate_gaia_starlight_release_inputs`

**Use when:** preparing the complete Gaia metadata and normalized input bundle
required by downstream starlight release processing.

**Inputs:** query policy, output directory, retrieval mode, magnitude and band
limits, license policy, validation reference, and candidate/production mode.

**Outputs:** ADQL, metadata, normalized chunks, checksums, diagnostics, and
release-input configuration.

**Resume/idempotency:** `--resume` preserves only already verified downloads and
normalized chunks.

**Failure contract:** production mode fails on missing XP products, parse
failures, missing provenance, or unreconciled source counts.

### `download_gaia_xp_continuous_bulk`

**Use when:** downloading official Gaia DR3 XP continuous bulk partitions from a
pinned inventory.

**Inputs:** official inventory, destination, storage policy, requested subset,
and retry configuration.

**Outputs:** checksum-verified partitions and acquisition manifest.

**Resume/idempotency:** explicit resume reuses only verified complete partitions
and continues incomplete downloads.

**Failure contract:** non-zero for missing inventory entries, truncated or
corrupted files, checksum mismatch, storage-policy failure, or exhausted retries.

### `index_gaia_xp_continuous_bulk`

**Use when:** building deterministic partition/source lookup and planning data
from verified XP continuous bulk files.

**Inputs:** checksum-verified official partitions and their inventory.

**Outputs:** versioned partition/source index and diagnostics.

**Resume/idempotency:** verified index partitions may be reused; full rebuilds
are deterministic.

**Failure contract:** non-zero when indexed partitions, source counts, or
checksums do not reconcile with the inventory.

## Gaia XP continuous contract

### `normalize_xp_continuous_coefficients`

**Use when:** converting raw official bulk or Gaia DataLink XP continuous
coefficient records into the canonical Rust schema.

**Inputs:** coefficient records, errors and correlations, source identity, and
provenance.

**Outputs:** canonical coefficient records and a manifest with exact rejection
accounting.

**Resume/idempotency:** deterministic normalization. Callers may reuse existing
outputs only after verifying their manifest and checksums.

**Failure contract:** non-zero for invalid coefficient dimensions, malformed
packed correlations, missing provenance, checksum mismatch, or inconsistent
accept/reject accounting.

### `validate_xp_continuous_reconstruction`

**Use when:** validating reconstructed XP continuous spectra, integrated photon
flux, uncertainty, and calibration provenance against the frozen contract.

**Inputs:** canonical coefficients, reconstructed results, calibration fixture
and provenance, and validation tolerances.

**Outputs:** versioned parity and validation report.

**Resume/idempotency:** read-only and deterministic.

**Failure contract:** non-zero when spectral, integrated-flux, uncertainty, or
provenance tolerances fail.

## Sampling and model development

### `generate_starlight_sample_queries`

**Use when:** generating the reproducible stratified Gaia query set used to build
or validate photometric models.

**Inputs:** sampling policy, population definitions, strata, and output
directory.

**Outputs:** deterministic ADQL query set and versioned sampling manifest.

**Resume/idempotency:** deterministic regeneration; no resume state.

**Failure contract:** non-zero if a required stratum is missing or the query set
and manifest are inconsistent.

### `consolidate_gaia_starlight_samples`

**Use when:** turning completed sample-query jobs into canonical, deduplicated,
spatially split modelling datasets.

**Inputs:** persisted TAP job manifests and results plus the frozen split
specification.

**Outputs:** canonical samples, inventory, exclusions, and split diagnostics.

**Resume/idempotency:** reuses verified completed TAP results and recomputes the
canonical consolidation deterministically.

**Failure contract:** non-zero for missing strata, duplicate sources,
unreconciled counts, invalid job results, or a split that violates the frozen
policy.

### `train_starlight_photometry_models`

**Use when:** developing and comparing candidate photometric transformations
from frozen train, validation, and test samples.

**Inputs:** canonical spatial splits and a versioned modelling policy.

**Outputs:** candidate coefficients, diagnostics, and holdout metrics.

**Resume/idempotency:** deterministic for fixed input checksums and policy; no
implicit resume.

**Failure contract:** non-zero for invalid datasets or failed requested model
checks. A successful run remains experimental until independent validation and
production admission are complete.

## Starlight generation and validation

### `build_starlight_map`

**Use when:** generating one deterministic full-sky HEALPix map from a canonical
source catalogue.

**Inputs:** canonical catalogue, HEALPix resolution and ordering, catalogue
provenance, checksums, and requested diagnostics.

**Outputs:** HEALPix CSV data product and optional versioned diagnostics report.

**Resume/idempotency:** no hidden resume; regenerate to a new path or replace an
output intentionally.

**Failure contract:** non-zero for malformed input, incomplete sky coverage,
failed required diagnostics, inconsistent flux accounting, or I/O failure.

### `sweep_starlight_nside`

**Use when:** building several HEALPix resolutions or reassessing existing sweep
artifacts to select a candidate resolution.

**Inputs:** canonical catalogue or persisted artifacts, catalogue checksum,
resolution policy, and validation references.

**Outputs:** per-resolution maps/diagnostics and a versioned sweep summary.

**Resume/idempotency:** `--assess-existing` reevaluates persisted artifacts
without rereading and rebuilding the catalogue.

**Failure contract:** non-zero when an explicitly requested candidate or
production condition is not satisfied. A candidate recommendation is not a
production admission decision.

### `validate_starlight_map`

**Use when:** producing the structural, scientific, and independent-reference
report for a generated map.

**Inputs:** generated map, provenance metadata, scientific policy, and validation
reference.

**Outputs:** versioned report with explicit passes, failures, `NotRun` gates, and
production blockers.

**Resume/idempotency:** read-only and deterministic.

**Failure contract:** non-zero for invalid maps, missing evidence, failed
requested gates, or an incomplete production validation request.

### `audit_gaia_starlight_exclusions`

**Use when:** reconciling the scientific exclusions sidecar against the canonical
Gaia source inventory.

**Inputs:** canonical catalogue, exclusions sidecar, rejection policy, and source
checksums.

**Outputs:** versioned exclusion audit and exact reconciliation evidence.

**Resume/idempotency:** read-only and deterministic.

**Failure contract:** non-zero unless every exclusion is unique, justified, and
accounted for exactly.

### `build_integrated_starlight_product`

**Use when:** combining approved population contributions into an integrated
starlight candidate and evaluating remaining production blockers.

**Inputs:** population contributions, frozen policies, validation references,
provenance, and output configuration.

**Outputs:** integrated candidate map, diagnostics, approval evidence, and
release metadata.

**Resume/idempotency:** no hidden resume; reusable persisted inputs are supplied
explicitly.

**Failure contract:** the command exits successfully only for the explicitly
requested candidate or production condition. Production admission fails closed
while any required gate or blocker remains unresolved.

## Commands that are intentionally absent

Phase-numbered executables, one-off policy-freeze commands, pilot runners, shell
wrappers, and migration-only Python programs are not supported command surfaces.
Their reusable algorithms belong in Rust library modules and services; their
scientific evidence belongs in frozen fixtures or historical documentation.

Do not reintroduce a historical development step as a new command. A new binary
must represent a durable capability with a real audience, be implemented as a
thin adapter, and be registered with complete contracts.