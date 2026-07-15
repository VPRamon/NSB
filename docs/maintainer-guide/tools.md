# Data-tool reference

Status: Current human-readable reference for `nsb-data-tools`.
Audience: Release maintainers, scientific-data maintainers, researchers, and external verifiers.
Scope: Purpose, audience, maturity, inputs, outputs, resume behaviour, and failure semantics of every compiled command.

The normative inventory is
[`crates/nsb-data-tools/tool-registry.toml`](../../crates/nsb-data-tools/tool-registry.toml).
It and `Cargo.toml` must contain the same 19 binaries.

```bash
cargo run --locked --release -p nsb-data-tools --bin <command> -- --help
```

Production-facing commands fail closed on missing provenance, incomplete
validation, checksum mismatch, schema errors, or unreconciled counts.

## Command overview

| Command | Area | Audience | Status |
| --- | --- | --- | --- |
| `verify_assets` | Asset verification | External users and release maintainers | Supported |
| `pack_starlight_asset` | Asset release | Release maintainers | Supported |
| `prepare_gaia_starlight_catalogue` | Catalogue preparation | Release maintainers | Supported |
| `prepare_tycho_starlight_catalogue` | Catalogue preparation | Researchers | Experimental |
| `query_gaia_tap` | Gaia acquisition | Release maintainers | Supported |
| `generate_gaia_starlight_release_inputs` | Gaia acquisition | Release maintainers | Supported |
| `download_gaia_xp_continuous_bulk` | Gaia acquisition | Release maintainers | Supported |
| `index_gaia_xp_continuous_bulk` | Gaia acquisition | Release maintainers | Supported |
| `normalize_xp_continuous_coefficients` | Gaia XP continuous | Release maintainers | Supported |
| `reconstruct_canonical_coefficients` | Gaia XP continuous | Release maintainers | Supported |
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

**Purpose:** Verify the runtime asset manifest, every referenced file, schema, checksum, and required metadata.

**Contract:** Asset manifest and referenced runtime files. → Verification report and process status.

**Execution:** Read-only and deterministic. **Failure:** Non-zero for missing/unregistered files, checksum mismatch, schema errors, or incomplete metadata.

### `pack_starlight_asset`

**Purpose:** Package an already validated starlight map together with its runtime manifest.

**Contract:** Validated map, provenance, validation evidence, checksums, and requested maturity. → Runtime map and manifest proposed for review or release.

**Execution:** Deterministic from immutable inputs; no implicit resume. **Failure:** Non-zero when admission evidence, provenance, headers, checksums, or maturity claims are incomplete.

## Catalogue preparation

### `prepare_gaia_starlight_catalogue`

**Purpose:** Convert official Gaia XP sampled data into canonical passband-integrated starlight source rows.

**Contract:** Official Gaia input, provenance, passband policy, and output paths. → Canonical catalogue, exclusions, and diagnostics.

**Execution:** Streaming deterministic conversion; callers manage verified inputs. **Failure:** Non-zero for malformed spectra, invalid photometry, missing provenance, or unreconciled source accounting.

### `prepare_tycho_starlight_catalogue`

**Purpose:** Convert Tycho rows into the canonical starlight schema for controlled studies.

**Contract:** Tycho catalogue and verified provenance. → Canonical catalogue and diagnostics.

**Execution:** Deterministic full conversion; no implicit resume. **Failure:** Non-zero for checksum, schema, coordinate, or photometry failure.

## Gaia acquisition and indexing

### `query_gaia_tap`

**Purpose:** Execute a reproducible Gaia TAP job with persisted request and result evidence.

**Contract:** ADQL, endpoint, output paths, and retry policy. → Verified TAP result and versioned job manifest.

**Execution:** Explicit resume continues the persisted asynchronous job or reuses a verified result. **Failure:** Non-zero for terminal service errors, invalid results, corrupted state, or exhausted retries.

### `generate_gaia_starlight_release_inputs`

**Purpose:** Generate the Gaia query/input bundle and provenance evidence required by downstream release processing.

**Contract:** Query policy, retrieval configuration, release metadata, and output directory. → Queries, normalized inputs, checksums, diagnostics, and release configuration.

**Execution:** Explicit resume reuses only verified persisted inputs. **Failure:** Non-zero when acquisition, provenance, source accounting, or requested production gates are incomplete.

### `download_gaia_xp_continuous_bulk`

**Purpose:** Download official Gaia XP continuous bulk partitions with checksum verification.

**Contract:** Official inventory, destination, requested subset, and retry policy. → Verified partitions and acquisition manifest.

**Execution:** Explicit resume reuses checksum-verified partitions and resumable partial state. **Failure:** Non-zero for inventory errors, corruption, checksum mismatch, storage failure, or exhausted retries.

### `index_gaia_xp_continuous_bulk`

**Purpose:** Build deterministic partition/source lookup data from verified XP continuous partitions.

**Contract:** Checksum-verified official bulk partitions. → Partition/source index and diagnostics.

**Execution:** Rebuilds are deterministic; verified index artifacts may be replaced intentionally. **Failure:** Non-zero when partition, source-count, or checksum reconciliation fails.

## Gaia XP continuous

### `normalize_xp_continuous_coefficients`

**Purpose:** Normalize official Gaia XP continuous coefficient records into the canonical Rust schema.

**Contract:** Raw coefficient records and source provenance. → Canonical coefficient records and exact accounting manifest.

**Execution:** Deterministic normalization; callers may reuse only verified canonical outputs. **Failure:** Non-zero for invalid dimensions, correlations, provenance, checksums, or accept/reject accounting.

### `reconstruct_canonical_coefficients`

**Purpose:** Reconstruct calibrated spectra, integrated photon flux, and uncertainty entirely in-process in Rust.

**Contract:** Canonical coefficients and the frozen design-matrix fixture. → Normalized spectra and provenance-rich reconstruction manifest.

**Execution:** Existing outputs are reused only when explicitly present and checksummed. **Failure:** Non-zero when parsing, calibration, integration, uncertainty, or manifest publication fails.

### `validate_xp_continuous_reconstruction`

**Purpose:** Validate XP continuous reconstruction against the frozen scientific contract.

**Contract:** Canonical coefficients, reconstructed results, calibration provenance, and tolerances. → Versioned parity and validation report.

**Execution:** Read-only and deterministic. **Failure:** Non-zero when spectral, flux, uncertainty, or provenance gates fail.

## Sampling and model development

### `generate_starlight_sample_queries`

**Purpose:** Generate deterministic stratified Gaia sampling queries.

**Contract:** Sampling policy, population definitions, and output directory. → ADQL query set and sampling manifest.

**Execution:** Deterministic regeneration; no implicit resume. **Failure:** Non-zero when required strata or manifest/query consistency checks fail.

### `consolidate_gaia_starlight_samples`

**Purpose:** Validate, deduplicate, and spatially split completed Gaia sampling results.

**Contract:** Persisted TAP results and frozen split specification. → Canonical samples, exclusions, inventory, and split diagnostics.

**Execution:** Reuses verified completed TAP results and recomputes consolidation deterministically. **Failure:** Non-zero for missing strata, duplicate sources, invalid results, or split/accounting failures.

### `train_starlight_photometry_models`

**Purpose:** Train candidate starlight photometry models from frozen datasets.

**Contract:** Canonical train, validation, and test samples plus modelling policy. → Candidate model and holdout diagnostics.

**Execution:** Deterministic for fixed inputs and policy; no implicit resume. **Failure:** Non-zero for invalid datasets or failed requested model checks.

## Starlight generation and validation

### `build_starlight_map`

**Purpose:** Build one deterministic HEALPix starlight map from a canonical catalogue.

**Contract:** Canonical catalogue, map configuration, provenance, and checksums. → Starlight map and diagnostics.

**Execution:** Deterministic from explicit inputs; no implicit resume. **Failure:** Non-zero for malformed input, incomplete accounting, failed required diagnostics, or I/O errors.

### `sweep_starlight_nside`

**Purpose:** Evaluate starlight map candidates across HEALPix resolutions.

**Contract:** Canonical catalogue or explicitly supplied verified candidate artifacts. → Per-resolution artifacts and sweep summary.

**Execution:** Assessment mode reuses explicitly supplied verified artifacts. **Failure:** Non-zero when an explicitly requested candidate or production condition fails.

### `validate_starlight_map`

**Purpose:** Validate a generated map against structural, scientific, and independent-reference requirements.

**Contract:** Map, provenance, scientific policy, and validation references. → Versioned validation report with pass, fail, not-run, and blocker evidence.

**Execution:** Read-only and deterministic. **Failure:** Non-zero for invalid maps, missing evidence, failed requested gates, or incomplete production validation.

### `audit_gaia_starlight_exclusions`

**Purpose:** Audit starlight exclusions against canonical source accounting.

**Contract:** Canonical catalogue, exclusions sidecar, rejection policy, and checksums. → Versioned exclusion audit and reconciliation evidence.

**Execution:** Read-only and deterministic. **Failure:** Non-zero unless every exclusion is unique, justified, and reconciled.

### `build_integrated_starlight_product`

**Purpose:** Build an integrated starlight candidate from approved contribution inputs and record admission blockers.

**Contract:** Population contributions, policies, validation references, provenance, and output configuration. → Integrated candidate, diagnostics, approval evidence, and release metadata.

**Execution:** Reusable persisted inputs are supplied explicitly; no hidden resume. **Failure:** Non-zero unless the explicitly requested candidate or production condition is satisfied.

## Removed command surfaces

Phase-numbered executables, one-off policy-freeze/finalization commands, pilot
runners, shell wrappers, and Python data-product programs are not supported
commands. Reusable algorithms belong in Rust modules and services; historical
evidence belongs in frozen fixtures or clearly labelled reference documentation.

A new binary must represent a durable capability, use a thin executable adapter,
and be added to `Cargo.toml`, `tool-registry.toml`, and this reference together.
