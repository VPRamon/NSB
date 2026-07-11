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

## Gaia DR3 release pipeline (validated 2026-07-11)

The production path for the real Gaia DR3 XP release uses the official sampled
bulk inventory, the fixed 336–650 nm photon-radiance contract
(`gaia_dr3_xp_photon_radiance_336_650nm_v1`), and an nside sweep that selects
**candidate** `nside=256`. Production promotion remains blocked until reviewed
external reference, missing-flux assessment, and redistribution policy gates are
satisfied.

### 1. Prepare canonical sources from official bulk

```bash
OUT="$HOME/nsb-data/starlight-gaia-release"
BULK="$OUT/gaia_dr3_xp_sampled_bulk"
LICENSE='Gaia DR3 data are open and free to use with credit to ESA/Gaia/DPAC; NSB redistributes only a derived validated runtime starlight map'

cargo run --locked --release -p nsb-data-tools --bin prepare_gaia_starlight_catalogue -- \
  --bulk-dir "$BULK" \
  --output "$OUT/gaia_dr3_starlight_sources.csv" \
  --diagnostics-output "$OUT/gaia_dr3_starlight_sources.diagnostics.json" \
  --exclusions-output "$OUT/gaia_dr3_starlight_exclusions.csv" \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$LICENSE" \
  --photometry-model "gaia_dr3_xp_photon_radiance_336_650nm_v1" \
  --band-min-nm 336 \
  --band-max-nm 650
```

Do not regenerate the validated 4.7 GiB canonical catalogue unless the bulk
inventory or scientific contract changes. Canonical SHA-256:

```text
1ad31ac492cc85c9e7b777c96f905fc27290265f4d2d7d65870021a72217cf30
```

### 2. Regenerate only the scientific-exclusions sidecar

Re-reads bulk; does not rewrite the canonical catalogue or rebuild maps:

```bash
cargo run --locked --release -p nsb-data-tools --bin prepare_gaia_starlight_catalogue -- \
  --bulk-dir "$BULK" \
  --exclusions-only \
  --exclusions-output "$OUT/gaia_dr3_starlight_exclusions.csv" \
  --diagnostics-output "$OUT/gaia_dr3_starlight_exclusions.diagnostics.json" \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$LICENSE" \
  --photometry-model "gaia_dr3_xp_photon_radiance_336_650nm_v1" \
  --band-min-nm 336 \
  --band-max-nm 650
```

Expected for the validated run: `rows_scientifically_excluded = 10`,
`rows_unexpectedly_rejected = 0`.

### 3. Nside sweep and reassessment

Full sweep rebuilds maps. Reassessment reuses persisted artefacts:

```bash
SWEEP="$OUT/sweep"
CATALOG_SHA="1ad31ac492cc85c9e7b777c96f905fc27290265f4d2d7d65870021a72217cf30"

cargo run --locked --release -p nsb-data-tools --bin sweep_starlight_nside -- \
  --output-dir "$SWEEP" \
  --assess-existing \
  --catalog-checksum "sha256:$CATALOG_SHA"
```

Observed candidate outcome: `recommended_candidate_nside = 256`,
`candidate_recommendation_passed = true`, `production_ready = false`.

### Normalized DataLink fallback (repair / controlled validation only)

The normalized one-row-per-source DataLink CSV (`--input`) remains available for
controlled validation or repair when the official bulk inventory is unavailable.
It is not the primary workflow for the validated 2026-07-11 release.

```bash
cargo run --locked -p nsb-data-tools --bin generate_gaia_starlight_release_inputs -- \
  --out-dir "$OUT" \
  --max-g-mag 20 \
  --license-policy-file docs/policies/gaia_dr3_starlight_derived_product_policy.txt \
  --validation-reference validation/starlight_independent_reference_v1.json \
  --xp-retrieval gaia-datalink
```

## Legacy Gaia DR3 release pipeline (obsolete)

The following block described an earlier 330–650 nm, `nside=128`, DataLink-first
workflow. It is retained only as historical context and must not be used for the
current release:

```bash
# OBSOLETE — do not use for the validated Gaia DR3 XP release
OUT=target/starlight-gaia-release
NSIDE=128
# photometry_model: gaia_dr3_xp_photon_radiance_330_650nm_v1
# band: 330–650 nm
# primary input: gaia-datalink normalized extract
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
