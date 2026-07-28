# Starlight dataset validation

`nsb-data dataset starlight validate --config <run.toml>` is the only supported
validation entry point. It verifies the built artifact set, immutable
checksums, required HEALPix headers and the configured scientific gates. The
versioned JSON report is stored in the configured workspace and is required by
`publish`.

Validation fails closed when an artifact is absent, changed, malformed,
incomplete or belongs to another dataset. Publication recomputes checksums so a
file changed after validation cannot enter `crates/nsb/data`.

Candidate map schema `nsb-healpix-starlight-candidate-v2` requires Galactic
NESTED HEALPix headers, `flux_quantity=integrated_per_pixel`,
`flux_unit=ph_m-2_s-1`, and resolution-specific derivation and source-count
semantics. Unknown, missing, or incompatible header values are rejected.

Two independent gates cover the complete 64/128/256/512 sweep:

- `cross-resolution-flux-conservation` recomputes compensated flux totals from
  each emitted map, compares them with report schema v3, checks every NESTED
  parent-child relation, and requires relative drift from nside 128 of at most
  0.1%. Non-finite, negative, missing, multiplied, or report-inconsistent
  products fail.
- `cross-resolution-source-accounting` recomputes both integer source totals,
  requires exact equality with nside 128 and the report, and checks exact
  conservative aggregation/apportionment for every parent.

The nside-256 and nside-512 child counts are bookkeeping only; validation does
not interpret their placement as recovered source locations.

Scientific production admission remains stricter than structural validity. A
production map also requires catalogue provenance, calibrated non-proxy
photometry, population accounting, coverage, finite non-negative integrated
pixel flux,
longitude wrapping, plane/pole behaviour, flux conservation where applicable,
independent comparison evidence and redistribution approval. These gates are
defined in [science requirements](science-requirements.md) and the [external
manifest](external-manifest.md).
