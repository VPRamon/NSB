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
product to bundle. The validated-external outcome remains available for
integrators and for testing the same runtime admission contract.
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
OUT=target/starlight-gaia-release
POLICY=docs/policies/gaia_dr3_starlight_derived_product_policy.txt
REF=validation/starlight_independent_reference_v1.json
LICENSE="<reviewed-derived-product-policy-string>"
DATE_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)
NSIDE=128

cargo run --locked -p nsb-data-tools --bin generate_gaia_starlight_release_inputs -- \
  --out-dir "$OUT" \
  --max-g-mag 20 \
  --production \
  --license-policy-file "$POLICY" \
  --validation-reference "$REF" \
  --xp-retrieval gaia-datalink

GAIA_DR3_STARLIGHT_EXTRACT="$OUT/gaia_dr3_starlight_extract.csv"
GAIA_DR3_STARLIGHT_EXTRACT_SHA256="sha256:$(sha256sum "$GAIA_DR3_STARLIGHT_EXTRACT" | cut -d' ' -f1)"

cargo run --locked -p nsb-data-tools --bin prepare_gaia_starlight_catalogue -- \
  --input "$GAIA_DR3_STARLIGHT_EXTRACT" \
  --output "$OUT/gaia_dr3_starlight_sources.csv" \
  --diagnostics-output "$OUT/gaia_dr3_starlight_sources.diagnostics.json" \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$LICENSE" \
  --source-checksum "$GAIA_DR3_STARLIGHT_EXTRACT_SHA256" \
  --photometry-model "gaia_dr3_xp_photon_radiance_330_650nm_v1" \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --require-passband-photometry

GAIA_DR3_STARLIGHT_SOURCES_SHA256="sha256:$(sha256sum "$OUT/gaia_dr3_starlight_sources.csv" | cut -d' ' -f1)"

cargo run --locked -p nsb-data-tools --bin build_starlight_map -- \
  --input "$OUT/gaia_dr3_starlight_sources.csv" \
  --output "$OUT/nsb_gaia_dr3_starlight_healpix.csv" \
  --diagnostics-output "$OUT/nsb_gaia_dr3_starlight_healpix.diagnostics.json" \
  --nside "$NSIDE" \
  --ordering ring \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$LICENSE" \
  --catalog-checksum "$GAIA_DR3_STARLIGHT_SOURCES_SHA256" \
  --photometry-model "gaia_dr3_xp_photon_radiance_330_650nm_v1" \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --generation-date-utc "$DATE_UTC" \
  --require-science-diagnostics

cargo run --locked -p nsb-data-tools --bin validate_starlight_map -- \
  --input "$OUT/nsb_gaia_dr3_starlight_healpix.csv" \
  --diagnostics "$OUT/nsb_gaia_dr3_starlight_healpix.diagnostics.json" \
  --reference "$REF" \
  --output "$OUT/nsb_gaia_dr3_starlight_healpix.validation.json" \
  --require-independent-comparison

cargo run --locked -p nsb-data-tools --bin pack_starlight_asset -- \
  --input "$OUT/nsb_gaia_dr3_starlight_healpix.csv" \
  --diagnostics "$OUT/nsb_gaia_dr3_starlight_healpix.diagnostics.json" \
  --validation "$OUT/nsb_gaia_dr3_starlight_healpix.validation.json" \
  --output "$OUT/nsb_gaia_dr3_starlight_healpix.release.csv" \
  --manifest "$OUT/nsb_gaia_dr3_starlight_healpix.manifest.toml" \
  --production
```

If redistribution policy permits bundling and the derived files are within the
release size budget, copy only the packed runtime files into `crates/nsb/data/`
as:

```text
crates/nsb/data/starlight_gaia_dr3_xp_330_650nm_nside128_v1.release.csv
crates/nsb/data/starlight_gaia_dr3_xp_330_650nm_nside128_v1.manifest.toml
```

Register the CSV with schema `nsb-healpix-starlight-v1` and the sidecar with
schema `nsb-starlight-runtime-manifest-v1`, both with
`calibration_status = "production"` and `runtime_embedded = true`. The `nsb`
build script then embeds both files, emits the
`nsb_bundled_production_starlight` cfg, and `ComponentMask::ALL` / CLI
`--components all` include the production starlight component.

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

### Official XP sampled bulk input

The preferred production input is the checksummed Gaia DR3 XP sampled bulk
inventory (`--bulk-dir`). Files are ECSV `*.csv.gz` tables with one source per
row. `flux` and `flux_error` are quoted bracketed arrays of 343 energy-flux
samples on the implicit 336–1020 nm grid (2 nm step). NSB integrates the
inclusive 336–650 nm band only; bulk rows do not carry a `wavelength` column.
Processing is streaming and fuses bulk SHA-256 verification with parsing so the
inventory is not read twice.

The normalized DataLink fallback (`--input`) remains for controlled validation or
resumable retrieval when the bulk inventory is unavailable.

### Nside sweep: candidate vs production

`sweep_starlight_nside` writes `summary.json` schema version 2 with separate
fields:

- `recommended_candidate_nside` — highest `nside` passing internal science and
  operational gates;
- `candidate_recommendation_passed` — automated candidate selection succeeded;
- `production_ready` — all production gates, including reviewed external
  reference and policy attestations;
- `production_blockers` — explicit reasons production remains blocked.

Regional independent comparison (`independent_regions_pass`) is separated from
reference approval (`independent_reference_production_use`). A provisional
internal envelope may therefore support candidate `nside` selection while
`production_ready` remains false.

Reassess persisted sweep directories without rebuilding maps:

```bash
cargo run --locked --release -p nsb-data-tools --bin sweep_starlight_nside -- \
  --output-dir "$HOME/nsb-data/starlight-gaia-release/sweep" \
  --assess-existing
```

Catalogue checksum verification during map builds uses streaming SHA-256; the
4.7 GiB canonical catalogue is not loaded entirely into memory.

The normalized DataLink fallback (`--input`) remains for controlled validation,
repair, or resumable retrieval when the bulk inventory is unavailable. It stores
semicolon-separated explicit wavelength series per source.

```bash
cargo run --locked -p nsb-data-tools --bin prepare_gaia_starlight_catalogue -- \
  --bulk-dir "$OUT/gaia_dr3_xp_sampled_bulk" \
  --output "$OUT/gaia_dr3_starlight_sources.csv" \
  --diagnostics-output "$OUT/gaia_dr3_starlight_sources.diagnostics.json" \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$LICENSE" \
  --photometry-model "gaia_dr3_xp_photon_radiance_336_650nm_v1" \
  --band-min-nm 336 \
  --band-max-nm 650
```

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

Until a redistributable product passes those gates, no bundled production
starlight asset is embedded and `ComponentMask::ALL` remains the non-starlight
planning set. Explicit production starlight requests fail closed unless a
validated external override is supplied; the experimental seed is never selected
as a fallback.
