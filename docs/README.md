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
- [NSB component guides](nsb_components/README.md)
- [Observatory configuration and customisation](user-guide/observatory-customization.md)

### Developers

- [Developer guide](developer-guide/README.md)
- [Architecture and modules](developer-guide/architecture.md)
- [Module reference](developer-guide/module-reference.md)
- [Performance contract](specifications/performance.md)
- [Logging contract](specifications/logging.md)
- [Siderust compatibility](specifications/siderust-compatibility.md)

### Maintainers

- [Maintainer guide](maintainer-guide/README.md)
- [Reproducible dataset workflow](maintainer-guide/datasets.md)
- [Data-product pipeline architecture](specifications/data-product-pipeline.md)
- [Release checklist](operations/release-checklist.md)

## Scientific interpretation and contracts

| Document | Purpose |
| --- | --- |
| [Scientific-model specification](specifications/scientific-model.md) | Physical quantities, query model, component composition, and window-search concepts |
| [Model-maturity specification](specifications/model-maturity.md) | Allowed scientific claims for every component and profile |
| [Scientific-metadata specification](specifications/scientific-metadata.md) | Provenance, maturity, uncertainty, validated domain, and diagnostic-band semantics |
| [Validation specification](specifications/validation.md) | Evidence, tolerances, limitations, and remaining validation gaps |
| [CTAO site-profile specification](specifications/ctao-site-profiles.md) | Exact assumptions and limitations of CTAO planning presets |
| [Jones 2013 moonlight](nsb_components/moonlight/jones2013-validation.md) | Spectral moonlight implementation and validation boundaries |
| [CLI schema specification](specifications/cli-schemas.md) | Stable JSON and CSV output contracts |

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
| [Starlight science requirements](nsb_components/starlight/science-requirements.md) | Required scientific properties and production gates |
| [Starlight generation](nsb_components/starlight/map-generation.md) | Current Gaia/Tycho candidate-generation workflow |
| [Starlight validation](nsb_components/starlight/map-validation.md) | Validation inputs, reports, gates, and failure modes |
| [External starlight manifest](nsb_components/starlight/external-manifest.md) | Fail-closed sidecar contract for external production maps |
| [Gaia DR3 ADQL](queries/gaia_dr3_starlight_extract.adql) | Recorded source-selection query |

A successful candidate build is not production admission. A bundled or external
product must satisfy provenance, checksum, scientific-validation, and maturity
contracts. The experimental seed is not a fallback for production requests.

## Documentation conventions

- User workflows live under `docs/user-guide/`.
- Architecture and extension guidance live under `docs/developer-guide/`.
- Data, release, and operational procedures live under `docs/maintainer-guide/`.
- Current cross-component contracts live under `docs/specifications/`.
- Component-specific science, generation, and validation live under
  `docs/nsb_components/`.
- Release procedures live under `docs/operations/`.
- Historical roadmaps, audits, migrations, and duplication registers are not
  retained as documentation; active requirements belong in their current
  specification.
- Rust public APIs are documented in rustdoc.
- Pages should state status, audience, scope, and important non-goals whenever
  misuse would affect scientific interpretation.
