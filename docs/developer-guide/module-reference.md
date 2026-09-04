# Module reference

Status: Current source-module inventory.
Audience: Developers and maintainers.
Scope: Responsibility and ownership of crate-level modules and important internal module groups.

This page maps source modules to their responsibility. Public API classification,
SemVer policy, and the snapshot gate are documented in
[Public API policy](public-api.md). Rustdoc remains the per-item reference.

## Workspace boundaries

| Crate | Responsibility | Runtime role |
| --- | --- | --- |
| `nsb` | Typed scientific models, component composition, point evaluation, threshold-window search, runtime assets, and scientific metadata | Linked by applications and the CLI |
| `nsb-cli` | User-facing parsing, site aliases, configuration templates, logging, and stable output rendering | Installed as the `nsb` executable |
| `nsb-data-tools` | Offline acquisition, transformation, validation, reconciliation, and packaging of scientific data | Maintainer-only; never invoked by runtime evaluation |
| `nsb-coverage-gate` | Overall and changed-production coverage gates over llvm-cov LCOV (JSON diagnostics) | CI/local quality tool; not a scientific runtime |
| `nsb-public-api-gate` | Public API snapshot integrity and historical SemVer gates via `cargo-public-api` | CI/local quality tool; not a scientific runtime |

## `nsb` modules

### Crate-level modules

| Module | Visibility | Responsibility |
| --- | --- | --- |
| `assets` | Public | Runtime asset-manifest access, checksum/provenance admission, and bundled-data selection |
| `components` | Public | Night-sky contributors and their typed inputs, outputs, metadata, and validation boundaries |
| `error` | Public | `NsbError` and the crate-wide `Result` alias |
| `evaluator` | Public | Evaluator construction, point queries, threshold queries, component selection, and result metadata |
| `site` | Public | Generic and named planning profiles, atmospheric assumptions, airglow scaling, and calibration status |
| `site_calibration` | Public | Evidence contract for dedicated site-calibration assets |
| `solar_activity` | Public | Offline F10.7 store loading and date-aware resolution |
| `spectrum` | Crate-private | Spectral integration and interpolation helpers |
| `units` | Crate-private with selected re-exports | NSB-specific typed quantities and scale-factor aliases |
| `window_search` | Internal | Adaptive interval scanning and threshold-crossing refinement |

### Component modules

| Module | Responsibility |
| --- | --- |
| `components::zodiacal` | Zodiacal brightness grid, solar reference spectrum, atmospheric extinction, and integrated outputs |
| `components::starlight` | HEALPix lookup, experimental/production separation, manifest validation, provenance, and diagnostics |
| `components::airglow` | Continuum model, seasonal/nightly/solar corrections, selectable emitting-volume geometry, independent Noll attenuation, and site scaling |
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
| `platform::artifact_store` | Durable temporary-write-and-rename persistence for generated artifacts and strict JSON state |
| `platform::checksum_io` | Algorithm-qualified checksums and integrity helpers |
| `platform::tool_logging` | Consistent maintainer-tool logging |

### Starlight products

| Module | Responsibility |
| --- | --- |
| `dataset::config` | Versioned TOML parsing and portable path resolution |
| `dataset::model` | Dataset, plan, artifact, validation, and run contracts |
| `dataset::pipeline` | Typed boundary between lifecycle infrastructure and dataset science |
| `dataset::engine` | Lifecycle, content-addressed run identity, integrity, recovery, reconciliation, and publication |
| `dataset::execution::scheduler` | Mockable Slurm submission and scheduler-state adapter |
| `dataset::slurm` | Slurm-array adapter for the shared Rust worker |
| `starlight::sources::inventory` | Strict normalization and pairing of the official GaiaSource and XP continuous inventories |
| `starlight::sources::acquisition` | Resumable verified downloads, content-addressed cache objects, and acquisition receipts |
| `starlight::map::accumulator` | Sparse HEALPix partition shards, exact accounting, and canonical-order reconciliation |

## Executable boundary

The sole executable and its four dataset workflows are documented in the
[dataset workflow](../maintainer-guide/datasets.md). Every extension must:

1. represent a durable capability;
2. keep reusable behaviour in its owning domain module;
3. define typed inputs and versioned outputs;
4. document resume/idempotency and exit-code semantics;
5. update the configuration contract and maintainer guide in the same change.

## `nsb-coverage-gate`

| Module | Responsibility |
| --- | --- |
| `check` | Overall workspace/`nsb` floors (fail-closed if `nsb` is missing) and changed-production diff evaluation |
| `lcov` | Parse `cargo llvm-cov report --lcov` (`DA:line,hits`) as the line-coverage source of truth |
| `llvm` | Optional `cargo llvm-cov --json` for function/region diagnostics |
| `diff` | `git diff -U0` / unified-diff changed-line extraction |
| `paths` | Production-file classification and crate ownership |
| `policy` | Load `coverage-policy.toml`; reject non-finite percents and nonempty `exclusions.files` |

See [Coverage policy](coverage.md).

## Keeping this reference current

Update this page whenever a crate-level module is added, removed, renamed, or
changes ownership. CI and rustdoc remain authoritative for public symbols; this
page is authoritative for architectural intent.
