# Starlight data-product pipeline

Integrated starlight is directional and catalogue-dependent, so NSB consumes a
Galactic HEALPix map generated offline. Runtime code never downloads catalogues.

## Current bundled seed

`starlight_galactic_map_v1.csv` is a 12-pixel, manually curated seed. It is
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

# Run the documented ADQL query and Gaia DataLink XP retrieval as a maintainer
# operation. The query recipe lives at docs/queries/gaia_dr3_starlight_extract.adql.

sha256sum target/starlight-release/gaia_dr3_starlight_extract.csv

cargo run --locked -p nsb-data-tools --bin prepare_gaia_starlight_catalogue -- \
  --input target/starlight-release/gaia_dr3_starlight_extract.csv \
  --output target/starlight-release/canonical_gaia_starlight_sources.csv \
  --diagnostics-output target/starlight-release/canonical_gaia_starlight_sources.diagnostics.json \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "<reviewed-license-or-derived-product-policy>" \
  --source-checksum "sha256:<raw-extract-checksum>" \
  --photometry-model "gaia_dr3_xp_photon_radiance_330_650nm_v1" \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --require-passband-photometry

sha256sum target/starlight-release/canonical_gaia_starlight_sources.csv

cargo run --locked -p nsb-data-tools --bin build_starlight_map -- \
  --input target/starlight-release/canonical_gaia_starlight_sources.csv \
  --output target/starlight-release/starlight_galactic_map_v1.csv \
  --diagnostics-output target/starlight-release/starlight_galactic_map_v1.diagnostics.json \
  --nside <chosen-nside> \
  --ordering ring \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "<reviewed-license-or-derived-product-policy>" \
  --catalog-checksum "sha256:<canonical-input-checksum>" \
  --photometry-model "gaia_dr3_xp_photon_radiance_330_650nm_v1" \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --generation-date-utc "<RFC3339 UTC>" \
  --require-science-diagnostics

cargo run --locked -p nsb-data-tools --bin validate_starlight_map -- \
  --input target/starlight-release/starlight_galactic_map_v1.csv \
  --diagnostics target/starlight-release/starlight_galactic_map_v1.diagnostics.json \
  --output target/starlight-release/starlight_galactic_map_v1.validation.json

cargo run --locked -p nsb-data-tools --bin pack_starlight_asset -- \
  --input target/starlight-release/starlight_galactic_map_v1.csv \
  --diagnostics target/starlight-release/starlight_galactic_map_v1.diagnostics.json \
  --validation target/starlight-release/starlight_galactic_map_v1.validation.json \
  --output crates/nsb/data/starlight_galactic_map_v1.bin.zst \
  --manifest crates/nsb/data/starlight_galactic_map_v1.manifest.toml
```

The packer emits a candidate framed asset and manifest. The current CI fixtures
do not provide independent astrophysical comparison evidence, so the bundled
production asset is still pending.

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
