# Independent Starlight validation

Scientific production readiness is established by reproducible,
checksum-pinned external validation. It does not require a separate manual
scientific signature. Redistribution/licensing remains a separate human/legal
decision and is not implied by scientific validation.

## Evidence routes

The literature-audit route retains three checksum-pinned targets (Toller,
Leinert, and GAMBONS). They are not admissible as starlight-only top-of-
atmosphere 300–650 nm grids, so the audit records
`no_admissible_independent_reference` and does not invent a transform.

The authoritative production-science route is
[`results/issue-207-external-cross-validation-v1.json`](results/issue-207-external-cross-validation-v1.json).
It pins:

- the exact NSB candidate SHA-256;
- `VPRamon/nsb-validation` and its metrics output;
- the `GerritRo/nsb2` reference commit and map checksum;
- preregistered acceptance bounds and region/tail metrics.

`nsb2` is cross-implementation evidence, not independent astrophysical ground
truth. Shared Gaia ancestry and different spectral/population models explain
why residual normalization differences must not be tuned away.

## Frozen inputs

- [`preregistration-v1.toml`](preregistration-v1.toml) freezes metric and
  tolerance vocabulary.
- [`references-v1.toml`](references-v1.toml) pins literature-reference bytes
  and admissibility assessments.
- [`regions-v1.json`](regions-v1.json) defines reproducible sky regions.

## Literature-audit command

```bash
nsb-data dataset starlight validation run \
  --preregistration docs/nsb_components/starlight/validation/preregistration-v1.toml \
  --references docs/nsb_components/starlight/validation/references-v1.toml \
  --regions docs/nsb_components/starlight/validation/regions-v1.json \
  --candidate-map crates/nsb/data/starlight_nside128.csv \
  --candidate-map-sha256 <expected-sha256> \
  --references-workspace <workspace> \
  --output <output>
```

This route independently reads and checksums the candidate, resolves regions,
and evaluates any admissible transformed references. Its machine-readable
`scientific_gate = "external_validation_required"` field means the literature
audit is evidence, not the production gate itself.

## Release policy

The release verifier accepts scientific readiness only when the external
validation artifact is checksum-pinned, identifies the final candidate,
pins the registered validator/reference commits, passes all numerical bounds,
and reports `passed = true`. CI and technical validation must also pass.

The sole remaining manual decision is
[`../release-candidate/redistribution-review-decision-v1.json`](../release-candidate/redistribution-review-decision-v1.json),
which remains `pending` until an authorized review occurs.
