# Module reference

Status: Current source-module inventory.
Audience: Developers and maintainers.
Scope: Responsibility and ownership of crate-level modules and important internal module groups.

This page maps source modules to their responsibility. Public API details remain
in rustdoc; this reference explains where behaviour belongs and which boundaries
must remain stable.

## Workspace boundaries

| Crate | Responsibility | Runtime role |
| --- | --- | --- |
| `nsb` | Typed scientific models, component composition, point evaluation, threshold-window search, runtime assets, and scientific metadata | Linked by applications and the CLI |
| `nsb-cli` | User-facing parsing, site aliases, configuration templates, logging, and stable output rendering | Installed as the `nsb` executable |
| `nsb-data-tools` | Offline acquisition, transformation, validation, reconciliation, and packaging of scientific data | Maintainer-only; never invoked by runtime evaluation |

## `nsb` modules

### Crate-level modules

| Module | Visibility | Responsibility |
| --- | --- | --- |
| `assets` | Public | Runtime asset-manifest access, checksum/provenance admission, and bundled-data selection |
| `components` | Public | Night-sky contributors and their typed inputs, outputs, metadata, and validation boundaries |
| `error` | Public | `NsbError` and the crate-wide `Result` alias |
| `evaluator` | Public | Evaluator construction, point queries, threshold queries, component selection, and result metadata |
| `site` | Public | Generic and named planning profiles, atmospheric assumptions, airglow scaling, and calibration status |
| `reference` | Internal | Shared immutable scientific reference inputs used by more than one component |
| `spectrum` | Crate-private | Spectral integration and interpolation helpers |
| `units` | Crate-private with selected re-exports | NSB-specific typed quantities and scale-factor aliases |
| `window_search` | Internal | Adaptive interval scanning and threshold-crossing refinement |

### Component modules

| Module | Responsibility |
| --- | --- |
| `components::zodiacal` | Zodiacal brightness grid, solar reference spectrum, atmospheric extinction, and integrated outputs |
| `components::starlight` | HEALPix lookup, experimental/production separation, manifest validation, provenance, and diagnostics |
| `components::airglow` | Continuum model, seasonal/nightly/solar corrections, Van Rhijn geometry, and site scaling |
| `components::moonlight` | Atmospheric inputs plus Jones 2013 spectral and Krisciunas–Schaefer 1991 reference models |

### Evaluator modules

| Module | Responsibility |
| --- | --- |
| `evaluator::types` | Query/result types, component masks, immutable model configuration, and model selectors |
| `evaluator::core` | Evaluator initialization and exact point composition |
| `evaluator::search` | Prepared threshold-search context, candidate periods, and orchestration |
| `evaluator::metadata` | Component maturity, provenance, validated domain, uncertainty, and diagnostic-band semantics |

## `nsb-cli` modules

| Module | Responsibility |
| --- | --- |
| `cli` | Clap definitions for global options and the `point`, `window`, `sites`, and `config` command families |
| `commands` | Thin handlers for `point`, `window`, `sites`, and `config` |
| `config` | Serializable configuration template and validation rules |
| `error` | User-facing CLI error classification and presentation |
| `logging` | Log-level resolution without contaminating machine-readable stdout |
| `output` | Stable table, JSON, and CSV rendering |
| `parsing` | Conversion of timestamps, coordinates, sites, components, thresholds, and model options into typed library values |

The CLI may translate and present library behaviour, but it must not implement
scientific models, coordinate algorithms, or threshold-search logic.

## `nsb-data-tools` modules

### Persistence, provenance, and common contracts

| Module | Responsibility |
| --- | --- |
| `artifact_io` | Transactional and atomic writing of generated artifacts |
| `checksum_io` | Algorithm-qualified checksums and integrity helpers |
| `provenance` | Stable provenance records for inputs, transformations, software identity, and outputs |
| `scientific_contract` | Versioned scientific-policy and schema contracts |
| `tool_logging` | Consistent maintainer-tool logging initialization |
| `tool_services` | Reusable command services called by thin `src/bin` adapters |

### Gaia acquisition

| Module | Responsibility |
| --- | --- |
| `gaia_tap` | Reproducible synchronous/asynchronous TAP jobs, retries, persisted manifests, and result validation |
| `gaia_datalink` | Gaia DataLink discovery and controlled retrieval |
| `gaia_bulk` | Official Gaia bulk inventory parsing and shared bulk-file operations |
| `gaia_bulk_service` | Typed service for checksum-verified bulk acquisition |

### Gaia XP sampled and continuous processing

| Module | Responsibility |
| --- | --- |
| `gaia_xp` | XP spectrum parsing and photon-flux integration primitives |
| `gaia_xp_continuous` | XP continuous record parsing, normalized spectra, constants, and shared contracts |
| `gaia_xp_continuous_canonical` | Strict canonical coefficient and bulk schemas plus streaming adapters |
| `gaia_xp_continuous_calibrate` | Pure-Rust calibration and uncertainty propagation from canonical coefficients |
| `gaia_xp_continuous_healpix` | HEALPix accumulation for reconstructed XP continuous contributions |
| `gaia_xp_continuous_bulk_schema` | Versioned official-inventory and partition schemas |
| `gaia_xp_continuous_bulk_index` | Deterministic source-to-partition indexing and lookup |
| `gaia_xp_continuous_pilot_io` | Checkpoint serialization and integrity helpers retained as reusable library support |

### Pipeline framework

| Module | Responsibility |
| --- | --- |
| `pipeline` | Shared persisted pipeline framework and public pipeline contracts |
| `pipeline::contracts` | Versioned modes, gates, outcomes, and admission types |
| `pipeline::checkpoint` | Checkpoint creation, validation, and recovery metadata |
| `pipeline::state` | Explicit state-machine transitions |
| `pipeline::store` | Persisted artifact and state storage abstraction |
| `pipeline::reconciliation` | Input/output/count reconciliation primitives |
| `pipeline::admission` | Fail-closed candidate and production admission decisions |

### Starlight products

| Module | Responsibility |
| --- | --- |
| `starlight_science` | Scientific constants, population definitions, passband policy, and shared validation rules |
| `starlight_sampling` | Deterministic strata, query generation, consolidation, and spatial splits |
| `starlight_approval` | Candidate review evidence and explicit production blockers |
| `starlight_integrated` | Population-contribution integration and final product construction |
| `starlight_phase5` | Frozen Phase 5 evidence readers and reconciliation retained for reproducibility |
| `starlight_phase5_holdout` | Frozen holdout definitions, independence checks, and preflight evidence |
| `starlight_phase5_uncertainty` | Frozen uncertainty calculations and evidence |

Phase-numbered **modules** preserve scientific evidence and reusable readers; the
phase-numbered executables have been removed. New workflows must use
capability-oriented commands and shared library services.

## Executable boundary

Compiled data tools are listed in
[`crates/nsb-data-tools/tool-registry.toml`](../../crates/nsb-data-tools/tool-registry.toml)
and documented in the [data-tool reference](../maintainer-guide/tools.md). Every
new executable must:

1. represent a durable capability;
2. keep reusable behaviour in a library module or `tool_services`;
3. define typed inputs and versioned outputs;
4. document resume/idempotency and exit-code semantics;
5. be added to the registry and maintainer reference in the same change.

## Keeping this reference current

Update this page whenever a crate-level module is added, removed, renamed, or
changes ownership. CI and rustdoc remain authoritative for public symbols; this
page is authoritative for architectural intent.
