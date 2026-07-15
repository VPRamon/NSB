# NSB documentation

Status: Current documentation hub.
Audience: Users, developers, maintainers, scientific reviewers, and integrators.
Scope: Navigation, project purpose, module boundaries, and authoritative references.

## What NSB is

NSB is a typed Rust library and command-line application for modelling the
ground-based night-sky background and finding observing periods that satisfy an
NSB threshold. It evaluates a configurable sum of zodiacal light, integrated
starlight, airglow, and atmospherically scattered moonlight for a specified
observer, UTC time, and target direction.

The repository separates three responsibilities:

| Crate | Purpose | Primary audience |
| --- | --- | --- |
| `nsb` | Scientific runtime library: typed queries, component models, point evaluation, window search, runtime assets, and maturity metadata | Rust integrators and developers |
| `nsb-cli` | Operational interface: commands, site aliases, parsing, output schemas, and logging | Users and automated workflows |
| `nsb-data-tools` | Offline generation, validation, reconciliation, and packaging of scientific data products | Maintainers and researchers |

Runtime evaluation never downloads catalogues or executes data-generation tools.
Scientific assets are prepared offline, validated, checksum-pinned, and admitted
through explicit runtime-manifest contracts.

## Choose your documentation path

### Users

- [User guide](user-guide/README.md)
- [Getting started](user-guide/getting-started.md)
- [Runtime components](user-guide/components.md)
- [Observatory configuration and customisation](user-guide/observatory-customization.md)

### Developers

- [Developer guide](developer-guide/README.md)
- [Architecture and modules](developer-guide/architecture.md)
- [Module reference](developer-guide/module-reference.md)
- [Performance contract](PERFORMANCE.md)
- [Logging contract](LOGGING.md)
- [Siderust compatibility](SIDERUST_COMPATIBILITY.md)

### Maintainers

- [Maintainer guide](maintainer-guide/README.md)
- [Updating scientific data](maintainer-guide/updating-data.md)
- [Data-product workflow](maintainer-guide/data-products.md)
- [Complete data-tool reference](maintainer-guide/tools.md)
- [Data-product pipeline architecture](DATA_PRODUCT_PIPELINE_ARCHITECTURE.md)
- [Pure-Rust Gaia XP continuous reconstruction](GAIA_XP_CONTINUOUS_RUST.md)
- [Release checklist](RELEASE_CHECKLIST.md)

## Scientific interpretation and contracts

| Document | Purpose |
| --- | --- |
| [Concepts and implementation](CONCEPTS_AND_IMPLEMENTATION_GUIDE.md) | Physical quantities, query model, component composition, and window-search concepts |
| [Model maturity](MODEL_MATURITY.md) | Allowed scientific claims for every component and profile |
| [Scientific metadata](SCIENTIFIC_METADATA.md) | Provenance, maturity, uncertainty, validated domain, and diagnostic-band semantics |
| [Validation matrix](VALIDATION.md) | Evidence, tolerances, limitations, and remaining validation gaps |
| [CTAO site profiles](CTAO_SITE_PROFILES.md) | Exact assumptions and limitations of CTAO planning presets |
| [Jones 2013 moonlight](moonlight_jones2013.md) | Spectral moonlight implementation and validation boundaries |
| [CLI schemas](CLI_SCHEMAS.md) | Stable JSON and CSV output contracts |

## Starlight data products

Use this reading order:

```text
science requirements
  -> input acquisition and catalogue preparation
  -> map generation
  -> validation
  -> production admission and packaging
  -> runtime manifest and maturity metadata
```

| Document | Purpose |
| --- | --- |
| [Starlight science requirements](STELLAR_MAP_SCIENCE_REQUIREMENTS.md) | Required scientific properties and production gates |
| [Starlight generation](STELLAR_MAP_GENERATION.md) | Current Gaia/Tycho candidate-generation workflow |
| [Starlight validation](STELLAR_MAP_VALIDATION.md) | Validation inputs, reports, gates, and failure modes |
| [External starlight manifest](EXTERNAL_STARLIGHT_MANIFEST.md) | Fail-closed sidecar contract for external production maps |
| [Gaia DR3 ADQL](queries/gaia_dr3_starlight_extract.adql) | Recorded source-selection query |

A successful candidate build is not production admission. A bundled or external
product must satisfy provenance, checksum, scientific-validation, and maturity
contracts. The experimental seed is not a fallback for production requests.

## Roadmaps, audits, and historical evidence

These documents support review but are not primary operational instructions:

- [Production roadmap](PRODUCTION_ROADMAP.md)
- [Gaia DR3 starlight science audit](GAIA_DR3_STARLIGHT_SCIENCE_AUDIT.md)
- [PR 56 starlight audit](STARLIGHT_PR56_AUDIT.md)
- [Phase 5 uncertainty contract](STARLIGHT_PHASE5_UNCERTAINTY.md)
- [XP continuous bulk notes](STARLIGHT_XP_CONTINUOUS_BULK.md)
- [Data-tool migration](DATA_TOOL_MIGRATION.md)
- [Duplication register](DUPLICATION_REGISTER.md)

Historical documents must be labelled clearly and must not override current
capability-oriented user and maintainer workflows.

## Documentation conventions

- User workflows live under `docs/user-guide/`.
- Architecture and extension guidance live under `docs/developer-guide/`.
- Data, release, and operational procedures live under `docs/maintainer-guide/`.
- Stable scientific contracts and evidence remain specialised references linked
  from the relevant guides.
- Rust public APIs are documented in rustdoc.
- Pages should state status, audience, scope, and important non-goals whenever
  misuse would affect scientific interpretation.
