# Starlight ultraviolet calibration contract

The Starlight UV interface is an ingestion, evaluation, and validation
contract for a future independently calibrated 300–336 nm correction. It is
not a trained calibration. The repository contains no production UV artifact,
reference spectra, or production coefficients. Consequently,
`crates/nsb-data-tools/config/starlight-production.toml` requests only the
directly measured Gaia XP 336–650 nm product.

Training and production use remain blocked until an immutable independently
flux-calibrated reference dataset, approved model specification, trained
artifact, holdout evidence, licence review, and scientific review are supplied.
The tooling deliberately does not invent a training algorithm.

## Contracts

All JSON objects are strict: unknown fields and unsupported `schema_version`
values are rejected.

The reference manifest has `schema_version = 1` and contains:

- the dataset name, release, licence, immutable file SHA-256 values, wavelength
  coverage, physical spectral-flux unit, transformations, and quality cuts;
- the checksum of the canonical source table;
- its source-ID and sky-region column names.

The partition manifest has `schema_version = 1`, a named deterministic
assignment algorithm and seed, and explicit `source_id`, `sky_region`, and
`partition` assignments. Each role (`training`, `validation`, and `test`) must
be non-empty. Source IDs cannot repeat, and a sky region cannot cross roles.
Materialization sorts by role, sky region, then source ID.

The correction artifact has `schema_version = 1` and contains:

- exact band `[300, 336]` and `ph_m-2_s-1` flux/statistical/systematic units;
- the complete reference-dataset identity;
- checksum-bound training/validation/test summaries and source/sky-disjoint
  evidence;
- ordered named predictors, transformations, and applicability limits;
- a model family, parameter vector, and covariance;
- statistical residual floor, systematic floor/fraction, explicit
  measured/correction statistical correlation, and explicit source-to-source
  systematic correlation;
- an out-of-domain rejection or boundary-clamping/conservative-uncertainty
  policy;
- validation metrics with colour, magnitude, extinction-proxy, quality, sky,
  and extrapolation strata;
- the exact training command, software version, model ID, and calibration
  status.

The currently supported evaluation family is `linear`: the first parameter is
the intercept and remaining parameters follow predictor order. Supporting this
serialization and evaluation family does not choose or train a scientific
model.

The canonical holdout is CSV with these required columns:

```text
source_id,expected_flux_300_336_ph_m2_s,colour,magnitude,extinction_proxy,quality,sky_region,<predictors...>
```

## Deterministic validation

Use the independently recorded artifact digest, not a digest learned from an
untrusted artifact:

```bash
export UV_ARTIFACT_SHA256='<digest from the approved immutable registry>'

cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  starlight-uv validate \
  --reference-manifest /calibration/reference-manifest.json \
  --partition-manifest /calibration/partitions.json \
  --materialize-partitions /validation/materialized-partitions.json \
  --artifact /calibration/uv-artifact.json \
  --artifact-sha256 "$UV_ARTIFACT_SHA256" \
  --holdout /calibration/holdout.csv \
  --output /validation/uv-validation-report.json
```

The deterministic report records the exact manifest, artifact, and holdout
checksums and aggregates residuals separately by colour, magnitude, extinction
proxy, quality, sky region, and in-domain/boundary/out-of-domain status.
Rejected rows remain counted and are never silently extrapolated.

## Runtime configuration

Only a `validated` artifact can be used by production workers. Its path is
resolved relative to the TOML file and its SHA-256 is verified before any
source is processed:

```toml
[starlight]
mode = "production"
product_band = "combined-300-650"

[starlight.ultraviolet_correction]
artifact_path = "/calibration/uv-artifact.json"
sha256 = "<approved immutable artifact digest>"
```

The digest marker above documents the field and is not a valid configuration:
validation requires exactly 64 lowercase hexadecimal characters. Requesting
`combined-300-650` without a valid artifact fails closed.

Workers reconstruct and integrate the measured 336–650 nm XP samples first.
They then evaluate the separate 300–336 nm correction from explicitly named
Gaia columns. XP spectral samples and `integrate_photon_flux` are never
extended below 336 nm. Shards and reports retain:

- `300–336 nm corrected`;
- `336–650 nm measured`;
- `300–650 nm combined`;
- correction and combined statistical uncertainty;
- correction systematic uncertainty with its declared correlation semantics;
- applicability status, model ID, artifact SHA-256, and calibration status.

Map pixels emit statistical and systematic uncertainty columns. Independent
source systematics accumulate as variance; fully correlated source systematics
accumulate linearly. No correlation choice is inferred when reading an
artifact.

The test-only miniature artifact under
`crates/nsb-data-tools/tests/fixtures/uv_synthetic_non_production/` is marked
`test-only` and `SYNTHETIC-NON-PRODUCTION` throughout. Production loading
rejects it.
