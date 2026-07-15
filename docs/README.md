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

Runtime evaluation never downloads catalogues or invokes data-generation tools.
Scientific assets are prepared offline, validated, checksum-pinned, registered in
the runtime manifest, and admitted through explicit contracts.

NSB is production-oriented software, but scientific maturity is
component-specific. The default model is suitable for deterministic planning; it
must not be described as site-calibrated unless returned metadata and validation
evidence support that claim.

## Choose your path

### Users

Start here to evaluate NSB, search observing windows, integrate the Rust API,
choose components, or configure an observatory.

1. [User guide](user-guide/README.md)
2. [Getting started](user-guide/getting-started.md)
3. [Runtime components](user-guide/components.md)
4. [Observatory configuration and customisation](user-guide/observatory-customization.md)

### Developers

Start here to change runtime models, CLI behaviour, data-product services, or
module boundaries.

1. [Developer guide](developer-guide/README.md)
2. [Architecture and modules](developer-guide/architecture.md)
3. [Complete module reference](developer-guide/module-reference.md)
4. [Performance contract](PERFORMANCE.md)
5. [Logging contract](LOGGING.md)
6. [Siderust compatibility](SIDERUST_COMPATIBILITY.md)

### Maintainers

Start here to update scientific data, operate the Gaia/starlight pipeline, verify
assets, or prepare a release.

1. [Maintainer guide](maintainer-guide/README.md)
2. [Updating scientific data](maintainer-guide/updating-data.md)
3. [Data-product workflow](maintainer-guide/data-products.md)
4. [Complete data-tool reference](maintainer-guide/tools.md)
5. [Data-product pipeline architecture](DATA_PRODUCT_PIPELINE_ARCHITECTURE.md)
6. [Release checklist](RELEASE_CHECKLIST.md)

## Scientific interpretation and contracts

These documents are authoritative for scientific meaning and review:

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

Use this reading order for integrated starlight work:

```text
science requirements
  -> source acquisition and catalogue preparation
  -> map or contribution generation
  -> validation and reconciliation
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

Production starlight is never inferred from a successful candidate build. A
bundled or external product must pass its complete provenance, checksum,
scientific-validation, and admission contract. The experimental manual seed is
not a fallback for production requests.

## Historical evidence and audits

The following material supports maintenance and scientific review but is not the
primary user workflow:

- [Production roadmap](PRODUCTION_ROADMAP.md)
- [Gaia DR3 starlight science audit](GAIA_DR3_STARLIGHT_SCIENCE_AUDIT.md)
- [PR 56 starlight audit](STARLIGHT_PR56_AUDIT.md)
- [Phase 5 uncertainty contract](STARLIGHT_PHASE5_UNCERTAINTY.md)
- [XP continuous bulk notes](STARLIGHT_XP_CONTINUOUS_BULK.md)
- [Data-tool migration](DATA_TOOL_MIGRATION.md)
- [Duplication register](DUPLICATION_REGISTER.md)

Historical pages may explain past decisions and frozen evidence, but they must
not override the current user, developer, maintainer, module, or tool references.
Phase-numbered commands are documented only where they remain compiled and are
explicitly classified as transitional.

## Documentation ownership

- User workflows live under `docs/user-guide/`.
- Architecture, module ownership, and extension guidance live under
  `docs/developer-guide/`.
- Data updates, tool operations, and release procedures live under
  `docs/maintainer-guide/`.
- Stable scientific contracts and evidence remain specialised reference pages
  linked from the relevant guide.
- Rust public APIs are documented in rustdoc.
- The data-tool registry is the machine-readable authority for compiled tools.

Every current page should state status, audience, and scope. Historical material
should state its date and limitations and must not be presented as current
operational guidance.