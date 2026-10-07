# Starlight validation

Status: Current validation contract and reproducibility inputs.

The checked-in validation directory defines the first public validation schema
and reference inputs. It does not contain a frozen pre-release candidate result.

## Inputs

- `preregistration-v1.toml` defines the candidate-map schema, required
  comparisons, metrics, and acceptance criteria.
- `references-v1.toml` records independently acquired scientific references
  and their checksums.
- `regions-v1.json` defines the sky regions used for structured comparisons.
- `acquired/` contains only redistribution-compatible reference material
  required by the checked-in validation contract.

## Running validation

A generated candidate is validated explicitly:

```bash
nsb-data dataset starlight validate --config <run.toml>
```

Cross-implementation or external scientific validation must additionally pin
the candidate SHA-256, validator identity, reference identity, transform
status, and resulting metrics. Those outputs are candidate-specific evidence
and are not committed as part of the 0.1.0 release baseline.

## Admission rule

Passing structural validation is necessary but not sufficient for production.
A production Starlight map also requires complete provenance, calibrated
photometry, uncertainty evidence, independent comparison, redistribution
approval, and an exact runtime sidecar accepted by
`ValidatedStarlightMap`.
