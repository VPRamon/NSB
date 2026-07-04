# Starlight data-product pipeline

Status: Maintainer workflow for candidate and release starlight map generation.
Audience: Maintainers preparing derived starlight artifacts.
Scope: Offline catalogue preparation, map generation, validation handoff, and
asset packing.
Non-goals: This document does not approve a generated map for production; see
[Starlight science requirements](STELLAR_MAP_SCIENCE_REQUIREMENTS.md) and
[Starlight map validation](STELLAR_MAP_VALIDATION.md).

Integrated starlight is directional and catalogue-dependent, so NSB consumes a
Galactic HEALPix map generated offline. Runtime code never downloads catalogues.

## Reading Path

```text
requirements -> generation -> validation -> packing -> maturity metadata
```

The requirements are in
[Starlight science requirements](STELLAR_MAP_SCIENCE_REQUIREMENTS.md). This file
describes how maintainers create map candidates. Validation report semantics are
defined in [Starlight map validation](STELLAR_MAP_VALIDATION.md). Caller-supplied
production maps use the separate
[external manifest contract](EXTERNAL_STARLIGHT_MANIFEST.md).

## Current bundled seed

`starlight_manual_seed_v1.csv` is a 12-pixel, manually curated seed. It is
registered as `experimental`, excluded from `ComponentMask::ALL`, and available
only through explicit experimental naming. It is not a production catalogue
product and cannot be promoted because its source selection is incomplete and
redistribution terms have not been reviewed.

Runtime loading is compile-time embedded, checksum-pinned, and checked against
manifest header expectations. Integrity does not imply scientific validity.

## Production-safe external path

NSB does not currently have a legally cleared, independently validated catalogue
product to bundle. Issue #45 therefore uses the validated-external outcome.
`ValidatedStarlightMap::from_files(map, manifest)` is the only caller-supplied
path that receives `Production` metadata. It requires a Galactic HEALPix map,
complete provenance, exact map checksum, an exact header contract, calibrated
non-proxy photometry, flux-conservation evidence, a validation report, and an
independent comparison. Runtime admission reruns complete-coverage,
finite/nonnegative, plane/pole, longitude-wrap, and (when source totals are
provided) flux-conservation checks.

The separate `StarlightModel::with_experimental_map(...)` API never receives a
production label. See [the sidecar schema](EXTERNAL_STARLIGHT_MANIFEST.md).

## Gaia DR3 release pipeline

Normal users do not download Gaia DR3 and do not provide source CSV files. The
Gaia extract and canonical source table are maintainer release artifacts. Only
the derived, checksum-pinned starlight map is intended to ship with NSB.

```bash
mkdir -p target/starlight-release

export GAIA_DR3_STARLIGHT_EXTRACT=target/starlight-release/gaia_dr3_starlight_extract.csv
export GAIA_DR3_STARLIGHT_EXTRACT_SHA256=sha256:<raw-extract-checksum>
export GAIA_DERIVED_PRODUCT_LICENSE_POLICY="<reviewed-license-or-derived-product-policy>"
export STARLIGHT_INDEPENDENT_VALIDATION_REFERENCE=target/starlight-release/independent_validation_reference.json
export STARLIGHT_GENERATION_NSIDE=128

# Run the documented ADQL query and Gaia DataLink XP retrieval as a maintainer
# operation. The query recipe lives at docs/queries/gaia_dr3_starlight_extract.adql.

sha256sum target/starlight-release/gaia_dr3_starlight_extract.csv

cargo run --locked -p nsb-data-tools --bin prepare_gaia_starlight_catalogue -- \
  --input "$GAIA_DR3_STARLIGHT_EXTRACT" \
  --output target/starlight-release/canonical_gaia_starlight_sources.csv \
  --diagnostics-output target/starlight-release/canonical_gaia_starlight_sources.diagnostics.json \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$GAIA_DERIVED_PRODUCT_LICENSE_POLICY" \
  --source-checksum "$GAIA_DR3_STARLIGHT_EXTRACT_SHA256" \
  --photometry-model "gaia_dr3_xp_photon_radiance_330_650nm_v1" \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --require-passband-photometry

sha256sum target/starlight-release/canonical_gaia_starlight_sources.csv

cargo run --locked -p nsb-data-tools --bin sweep_starlight_nside -- \
  --input target/starlight-release/canonical_gaia_starlight_sources.csv \
  --output-dir target/starlight-release/sweep \
  --reference "$STARLIGHT_INDEPENDENT_VALIDATION_REFERENCE" \
  --catalog-checksum "sha256:<canonical-input-checksum>" \
  --catalog-license "$GAIA_DERIVED_PRODUCT_LICENSE_POLICY" \
  --generation-date-utc "<RFC3339 UTC>"

cargo run --locked -p nsb-data-tools --bin build_starlight_map -- \
  --input target/starlight-release/canonical_gaia_starlight_sources.csv \
  --output target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.csv \
  --diagnostics-output target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.diagnostics.json \
  --nside "$STARLIGHT_GENERATION_NSIDE" \
  --ordering ring \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$GAIA_DERIVED_PRODUCT_LICENSE_POLICY" \
  --catalog-checksum "sha256:<canonical-input-checksum>" \
  --photometry-model "gaia_dr3_xp_photon_radiance_330_650nm_v1" \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --generation-date-utc "<RFC3339 UTC>" \
  --require-science-diagnostics

cargo run --locked -p nsb-data-tools --bin validate_starlight_map -- \
  --input target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.csv \
  --diagnostics target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.diagnostics.json \
  --reference "$STARLIGHT_INDEPENDENT_VALIDATION_REFERENCE" \
  --output target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.validation.json \
  --require-independent-comparison

cargo run --locked -p nsb-data-tools --bin pack_starlight_asset -- \
  --input target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.csv \
  --diagnostics target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.diagnostics.json \
  --validation target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.validation.json \
  --output crates/nsb/data/starlight_gaia_dr3_xp_330_650nm_nside128_v1.release.csv \
  --manifest crates/nsb/data/starlight_gaia_dr3_xp_330_650nm_nside128_v1.manifest.toml \
  --production
```

The packer emits a raw UTF-8 HEALPix release CSV and runtime manifest only in
`--production` mode after `production_ready=true`, integrated flux conservation
passes, the longitude seam diagnostic passes, and the validation tool has
computed passing structured independent regional comparisons. Production packing
self-loads the emitted CSV/TOML pair through the runtime `ValidatedStarlightMap`
loader before returning success. Boolean claims supplied by external reference
files are not trusted.
Use `--candidate` only for review artifacts. The current repository does not
ship the Gaia-derived production asset because the real Gaia extract, reviewed
redistribution policy, and independent validation reference are not present in
CI.

## Legacy Tycho proxy pipeline

```text
reviewed local Tycho-like release
  -> prepare_tycho_starlight_catalogue
  -> canonical CSV + JSON preparation diagnostics
  -> build_starlight_map using Siderust
  -> HEALPix CSV + JSON validation diagnostics
  -> manifest checksum/header update
  -> independent validation
  -> maturity review
```

Canonical input columns are:

```text
ra_deg,dec_deg,b_mag,v_mag,weight,source_id
```

The preparation tool expects `bt_mag`/`vt_mag` and labels its conversion
`tycho_bt_vt_to_johnson_bv_proxy_v1`. It computes and optionally verifies the
input SHA-256 and writes machine-readable counts, filters, catalogue metadata,
and maturity.

```bash
cargo run --locked -p nsb-data-tools --bin prepare_tycho_starlight_catalogue -- \
  --input tycho_extract.csv \
  --output catalogue_for_starlight.csv \
  --diagnostics-output catalogue_for_starlight.diagnostics.json \
  --catalog-name "Tycho-2" \
  --catalog-release "reviewed release" \
  --catalog-license "reviewed redistribution terms" \
  --input-checksum "sha256:<actual checksum>" \
  --max-v-mag 11.5
```

Map generation delegates coordinate transforms, HEALPix, map building, and
validators to Siderust:

```bash
cargo run --locked -p nsb-data-tools --bin build_starlight_map -- \
  --input catalogue_for_starlight.csv \
  --output starlight_galactic_map_candidate.csv \
  --diagnostics-output starlight_galactic_map_candidate.diagnostics.json \
  --nside 64 \
  --ordering ring \
  --max-v-mag 11.5 \
  --catalog-name "Tycho-2" \
  --catalog-release "reviewed release" \
  --catalog-license "reviewed redistribution terms" \
  --catalog-checksum "sha256:<canonical input checksum>" \
  --generation-date-utc "<actual RFC3339 UTC>" \
  --require-science-diagnostics
```

Production-style diagnostics require release, license, and checksum metadata.
The tool verifies the input checksum and hard-fails flux conservation,
plane/pole contrast, or longitude-wrap diagnostics when requested. Output JSON
contains counts, totals, empty pixels, pass/fail fields, photometry model, and
output checksum.

## Photometry limitation

The available Siderust builder uses
`v_s10_scaled_integrated_proxy_v1`. B/V values are diagnostics and the integrated
factor is not passband/spectral synthesis. A candidate remains experimental
until a passband-aware conversion and independent comparison are validated.

## Promotion gates

Promotion requires all of the following:

1. released source catalogue and immutable checksum;
2. reviewed license permitting the derived bundled product;
3. documented magnitude selection and completeness;
4. deterministic generation command and JSON report;
5. full-pixel, finite, nonnegative, flux, plane/pole, center/reference, wrap,
   and bright-region diagnostics;
6. independent astrophysical validation with units/bands/tolerances;
7. passband-aware integrated radiance or an explicitly non-production proxy;
8. manifest update and runtime metadata review.

Until a redistributable product passes those gates, starlight remains outside
`ComponentMask::ALL`. Production use fails closed around the external sidecar;
the experimental seed is never selected as a fallback.
