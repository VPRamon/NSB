# NSB Documentation

Status: Current for the `production-readiness-final` branch.
Audience: Users, scientific reviewers, CLI integrators, and maintainers.
Scope: A reading path through the project documentation and release evidence.
Non-goals: This page is not an API reference, validation report, or release
approval.

NSB separates production-oriented software behavior from scientific calibration.
The default model is suitable for deterministic planning workflows, but
component maturity must be read from metadata and validation evidence.

## Start Here

| Document | Audience | Purpose |
| --- | --- | --- |
| [Concepts and implementation](CONCEPTS_AND_IMPLEMENTATION_GUIDE.md) | New library and CLI users | Defines the physical quantities, query model, component composition, and runtime architecture. |
| [Model maturity](MODEL_MATURITY.md) | Scientific users and reviewers | Lists each component's maturity status and the production claims that are allowed. |
| [Scientific metadata](SCIENTIFIC_METADATA.md) | Users consuming API or CLI output | Explains maturity metadata, uncertainty fields, provenance, and B/V diagnostic limitations. |
| [Validation matrix](VALIDATION.md) | Reviewers and maintainers | Maps each scientific surface to its evidence, tolerance, and remaining limitations. |

## Scientific Interpretation

| Document | Audience | Purpose |
| --- | --- | --- |
| [CTAO site-profile assumptions](CTAO_SITE_PROFILES.md) | CTAO planning users and reviewers | Describes named CTAO presets and why they are not site-calibrated products. |
| [Jones 2013 spectral moonlight validation](moonlight_jones2013.md) | Moonlight model reviewers | Documents the Jones spectral implementation, atmospheric assumptions, fixtures, and accuracy limits. |
| [Performance contract](PERFORMANCE.md) | Maintainers and performance reviewers | States reuse boundaries, benchmark coverage, and acceptable performance changes without changing scientific output. |

## Starlight Data Products

Read these in order when reviewing starlight:

```text
science requirements
  -> generation pipeline
  -> validation report
  -> external manifest or bundled asset decision
  -> model maturity and validation matrix
```

| Document | Audience | Purpose |
| --- | --- | --- |
| [Starlight science requirements](STELLAR_MAP_SCIENCE_REQUIREMENTS.md) | Scientific reviewers and maintainers | Defines what must be true before any starlight product can be treated as production. |
| [Starlight data-product pipeline](STELLAR_MAP_GENERATION.md) | Maintainers | Shows how local catalogue extracts are converted into derived map candidates and review artifacts. |
| [Starlight map validation](STELLAR_MAP_VALIDATION.md) | Maintainers and reviewers | Specifies the validation harness, required inputs, report fields, gates, and failure modes. |
| [Validated external starlight manifest](EXTERNAL_STARLIGHT_MANIFEST.md) | Integrators supplying their own maps | Defines the sidecar contract for caller-provided production starlight maps. |
| [Gaia DR3 starlight ADQL query](queries/gaia_dr3_starlight_extract.adql) | Maintainers | Records the Gaia DR3 source-selection query used by the release input workflow. |

The bundled manual starlight seed is experimental. `ComponentMask::ALL` and CLI
`--components all` exclude starlight unless the caller also selects an explicit
starlight mode.

## CLI And Output Contracts

| Document | Audience | Purpose |
| --- | --- | --- |
| [Stable CLI schemas](CLI_SCHEMAS.md) | CLI consumers and downstream tooling | Describes JSON and CSV schema identifiers, stable fields, and audit metadata. |
| [Siderust compatibility](SIDERUST_COMPATIBILITY.md) | Maintainers and downstream packagers | Records the current Siderust dependency source, lockfile revision, update policy, and release requirements. |

## Maintainer And Release Material

| Document | Audience | Purpose |
| --- | --- | --- |
| [Production-readiness roadmap](PRODUCTION_ROADMAP.md) | Maintainers and release reviewers | Summarizes issue-level release workstreams and the distinction between software release and calibrated science release. |
| [Release checklist](RELEASE_CHECKLIST.md) | Release maintainers | Lists the checks that must pass before tagging or distributing a release. |

Historical architecture notes and migration reports were removed from the
working tree because they no longer matched current defaults. Their content
remains available in Git history.
