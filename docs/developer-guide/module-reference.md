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

### Platform

| Module | Responsibility |
| --- | --- |
| `platform::artifact_io` | Transactional and atomic writing of generated artifacts |
| `platform::checksum_io` | Algorithm-qualified checksums and integrity helpers |
| `platform::pipeline` | Persisted processing modes, gates, checkpoints, state, stores, reconciliation, and admission |
| `platform::tool_catalog` / `tool_logging` | Normative action registry plus consistent maintainer-tool logging |
| `platform::verify_assets` | Runtime scientific-asset verification action |

### Gaia acquisition

| Module | Responsibility |
| --- | --- |
| `gaia::acquisition::tap` | Reproducible synchronous/asynchronous TAP jobs, retries, persisted manifests, and result validation |
| `gaia::acquisition::datalink` | Gaia DataLink discovery and controlled retrieval |
| `gaia::acquisition::bulk` / `bulk_service` | Official inventory parsing and checksum-verified bulk acquisition |

### Gaia XP sampled and continuous processing

| Module | Responsibility |
| --- | --- |
| `gaia::xp::sampled` | XP spectrum parsing and photon-flux integration primitives |
| `gaia::xp::continuous` / `canonical` | XP continuous records, strict canonical schemas, and streaming adapters |
| `gaia::xp::calibrate` / `bulk_index` | Pure-Rust calibration, uncertainty propagation, and deterministic source-to-partition lookup |
| `gaia::xp::contract` | Versioned Gaia XP photon-integration contract and drift validation |

### Starlight products

| Module | Responsibility |
| --- | --- |
| `dataset::config` | Versioned TOML parsing and portable path resolution |
| `dataset::model` | Dataset, plan, artifact, validation, and run contracts |
| `dataset::engine` | Lifecycle, integrity, recovery, reconciliation, and publication |
| `dataset::slurm` | Slurm-array adapter for the shared Rust worker |
| `platform` | Streaming checksums and stderr logging |

## Executable boundary

The sole executable and its four dataset workflows are documented in the
[dataset workflow](../maintainer-guide/datasets.md). Every extension must:

1. represent a durable capability;
2. keep reusable behaviour in its owning domain module;
3. define typed inputs and versioned outputs;
4. document resume/idempotency and exit-code semantics;
5. update the configuration contract and maintainer guide in the same change.

## Keeping this reference current

Update this page whenever a crate-level module is added, removed, renamed, or
changes ownership. CI and rustdoc remain authoritative for public symbols; this
page is authoritative for architectural intent.
