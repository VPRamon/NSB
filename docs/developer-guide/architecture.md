# Architecture and modules

Status: Current high-level software architecture.
Audience: Developers and maintainers.
Scope: Crate boundaries, runtime data flow, module ownership, and extension points.

## System view

```text
user or scheduler
      |
      +--> nsb-cli -----------------------------------+
      |     parsing, site aliases, output, logging    |
      |                                               v
      +-------------------------------------------> nsb
                                                    typed queries
                                                    component models
                                                    point evaluation
                                                    window search
                                                    runtime metadata
                                                        ^
                                                        |
reviewed runtime assets <--- packaging/validation <--- nsb-data-tools
```

Runtime evaluation is local and deterministic. `nsb` does not download
catalogues or create scientific assets. `nsb-data-tools` produces reviewed
artifacts offline, and runtime builds admit only assets registered through the
manifest and build-time checks.

## `nsb`: runtime scientific library

### Public top-level modules

| Module | Responsibility |
| --- | --- |
| `assets` | Runtime asset registry access, manifest checks, and embedded scientific data selection |
| `components` | Physical and empirical contributors to the night-sky background |
| `error` | Typed library errors and the crate result alias |
| `evaluator` | Public orchestration layer for point queries and threshold-window searches |
| `site` | Built-in atmospheric and airglow profile metadata with explicit maturity |

Internal `reference`, `spectrum`, `units`, and window-search modules support the
public surface without becoming independent operational APIs.

### Component modules

| Component module | Role | Primary extension concern |
| --- | --- | --- |
| `components::zodiacal` | Directional zodiacal brightness, reference spectrum, and atmospheric extinction | Preserve grid/reference provenance and typed radiometry |
| `components::starlight` | HEALPix map lookup, provenance, validation, and runtime admission | Keep experimental and production paths strictly separate |
| `components::airglow` | Continuum, temporal/seasonal behaviour, solar activity, geometry, and site scaling | Preserve explicit calibration assumptions and time-domain tests |
| `components::moonlight` | Atmospheric conditions and scattered-moonlight models | Keep published-reference and spectral models distinguishable in metadata |

The evaluator composes components but does not erase their individual results or
metadata.

### Evaluator modules

| Module | Responsibility |
| --- | --- |
| `evaluator::types` | Query, result, model-configuration, component-mask, and model-selection types |
| `evaluator::core` | Evaluator construction and point composition |
| `evaluator::search` | Threshold-window orchestration and prepared search context |
| `evaluator::metadata` | Maturity, provenance, uncertainty, and diagnostic-band metadata |

`NsbModelConfig` selects immutable model choices when the evaluator is created.
`PointQuery` and `ThresholdQuery` carry per-query geometry, time, component, and
constraint inputs.

### Window-search flow

```text
UTC query
  -> typed time/coordinate preparation
  -> Sun and target visibility pre-filters
  -> Moon visibility periods when required
  -> airglow phase boundaries
  -> candidate subwindows
  -> adaptive samples
  -> bracketed threshold-crossing refinement
  -> UTC result periods
```

Query-wide event information is prepared once and reused. Exact component
evaluation remains authoritative at accepted samples and refined crossings.

## `nsb-cli`: operational interface

| Area | Responsibility |
| --- | --- |
| `cli` | Clap command and argument definitions for `point`, `window`, `sites`, and `config` |
| `parsing` | UTC, observer, target, component, radiance, and site-alias conversion into typed library values |
| `commands` | Thin command handlers that invoke the library or configuration utilities |
| `output` | Stable table, JSON, and CSV presentation |
| `config` | Serializable configuration schema used by `config init` and `config validate` |
| `logging` | Operational log-level resolution and initialization |

The CLI owns user-friendly aliases and formatting. It must not duplicate component
models, coordinate algorithms, or window-search logic.

## `nsb-data-tools`: offline scientific data products

### Architectural layers

```text
src/bin/nsb-data.rs
  -> cli/
      -> starlight/ actions and science
      -> gaia/ acquisition and XP primitives
      -> platform/ persistence, pipeline, logging, and registry
```

A binary parses arguments, initializes logging, constructs typed configuration,
calls one service, and maps the typed result to a stable exit status. Scientific
algorithms and persisted state machines belong below the executable boundary.

### Shared module groups

| Module group | Responsibility |
| --- | --- |
| `platform` | Streaming checksums and logging shared by the dataset engine |
| `dataset::config` | Versioned sources, workspace, execution, and publication configuration |
| `dataset::model` | Typed plans, artifacts, gates, reports, and run manifests |
| `dataset::engine` | Shared update/build/validate/publish lifecycle |
| `dataset::slurm` | Slurm submission of the same Rust partition worker |

Every exposed `nsb-data` operation has typed input/output, resume and failure
semantics. Domain-specific scripts, aliases and secondary executables are not
retained.

## Data and asset boundaries

Runtime data live under `crates/nsb/data/` and are registered in
`crates/nsb/data/manifest.toml`. The manifest is authoritative for file coverage,
checksums, provenance, license information, schema, and maturity.

At crate build time, `crates/nsb/build.rs` (with helpers in
`crates/nsb/build/`) parses that manifest, validates path confinement and
`runtime_embedded` assets (existence + SHA-256), enforces Starlight release
policy, and emits static Rust metadata consumed by `nsb::assets`. Runtime code
does not parse `manifest.toml`. Candidate or external assets
(`runtime_embedded = false`) may remain registered without becoming compile
requirements and are not exposed through the verified bundled-asset API.

Generated catalogues, maps, checkpoints, diagnostics, and reports belong in a
caller-selected output directory. They are not repository source until a
reviewed release explicitly admits the required runtime artifact and metadata.

## Dependency and ownership rules

- Siderust owns general astronomy, time, coordinates, events, atmosphere,
  ephemerides, passbands, and HEALPix primitives.
- NSB owns night-sky component composition, NSB-specific empirical data,
  planning-window behaviour, and maturity-bearing metadata.
- The CLI may depend on `nsb`; `nsb` must not depend on the CLI.
- Data tools may use scientific library code and Siderust primitives, but runtime
  evaluation must not invoke data tools.
- Scientific modules must not spawn binaries or shell pipelines.
- Persisted production schemas reject unknown fields and unsupported versions.

## Adding a feature

### New runtime component

1. Add a component module with typed inputs and outputs.
2. Define maturity, provenance, validated domain, and uncertainty metadata.
3. Add explicit selection in `ComponentMask` and evaluator composition.
4. Add CLI parsing only after the library contract is stable.
5. Add validation and performance evidence.
6. Update the user component guide and model maturity matrix.

### New CLI command

1. Keep the command handler thin.
2. Convert arguments to existing typed library inputs.
3. Version any new machine-readable output schema.
4. Add smoke tests and documentation.
5. Do not place scientific logic in the command module.

### New dataset capability

1. Extend the typed dataset contract instead of adding another executable.
2. Implement reusable behaviour below `dataset::engine`.
3. Keep CLI routing thin and dataset-oriented.
4. Document configuration, outputs, resume semantics and production gates.
5. Add lifecycle, corruption, recovery and publication tests.
