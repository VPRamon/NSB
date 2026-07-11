# nsb-data-tools

Offline, non-runtime tools for scientific data products.

- `generate_gaia_starlight_release_inputs`: maintainer-only Gaia DR3 release
  input generator. It writes the Gaia metadata query, optionally downloads Gaia
  metadata through TAP, merges parsed XP chunk files, computes the Gaia extract
  checksum, writes diagnostics, and emits a `starlight_release_inputs.env` file
  for the downstream starlight release pipeline. Runtime NSB never calls this
  tool.
- `prepare_tycho_starlight_catalogue`: converts local BT/VT catalogue rows to
  canonical rows, verifies input SHA-256, and emits JSON diagnostics. Its colour
  transform is explicitly experimental.
- `prepare_gaia_starlight_catalogue`: converts official Gaia DR3 XP sampled bulk
  files or a normalized DataLink fallback extract into canonical passband-
  integrated source rows using Siderust Gaia/passband APIs.
- `build_starlight_map`: delegates transforms, HEALPix, construction, and
  validators to Siderust; writes a complete map and optional JSON diagnostics.
  It supports both the legacy proxy B/V input and Gaia passband photon-flux
  source tables.
- `sweep_starlight_nside`: runs candidate map builds for multiple HEALPix
  resolutions, writes `summary.json` schema v2 with separate candidate and
  production gates, and supports `--assess-existing` to reevaluate persisted
  sweep artefacts without rebuilding maps.
- `validate_starlight_map`: emits a validation report for generated maps.
- `pack_starlight_asset`: writes a raw release HEALPix CSV and runtime TOML
  manifest for a derived bundled asset candidate or production asset.
- `verify_assets`: verifies the asset registry, required metadata, schemas,
  checksums, file coverage, and configured headers.

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

## Gaia DR3 release input generation

Smoke/candidate run for the legacy DataLink input generator (fallback path only):

```bash
cargo run --locked -p nsb-data-tools --bin generate_gaia_starlight_release_inputs -- \
  --out-dir target/starlight-smoke \
  --max-g-mag 12.0 \
  --limit 1000 \
  --chunk-size 100 \
  --band-min-nm 336 \
  --band-max-nm 650 \
  --candidate \
  --resume
```

Production-style DataLink retrieval (fallback when official bulk is unavailable):

```bash
cargo run --locked -p nsb-data-tools --bin generate_gaia_starlight_release_inputs -- \
  --out-dir target/starlight-release \
  --max-g-mag 20.0 \
  --chunk-size 5000 \
  --band-min-nm 336 \
  --band-max-nm 650 \
  --license-policy-file docs/policies/gaia_dr3_starlight_derived_product_policy.txt \
  --validation-reference validation/starlight_independent_reference_v1.json \
  --xp-retrieval gaia-datalink \
  --production \
  --resume
```

The tool writes:

```text
target/starlight-release/gaia_dr3_starlight_extract.adql
target/starlight-release/gaia_dr3_metadata.csv
target/starlight-release/gaia_dr3_xp_chunks/
target/starlight-release/gaia_dr3_starlight_extract.csv
target/starlight-release/gaia_dr3_starlight_extract.diagnostics.json
target/starlight-release/gaia_dr3_starlight_extract.sha256
target/starlight-release/gaia_derived_product_policy.txt
target/starlight-release/starlight_release_inputs.env
```

The XP chunk merger expects chunk files that expose `source_id`,
`xp_wavelength_nm`, and `xp_flux_w_m2_nm` as CSV columns or equivalent JSON
fields. If the Gaia DataLink response is stored in a different native layout,
keep the raw chunks and adapt the parser before using `--production`.

## Gaia DR3 XP sampled bulk preparation

Production preparation reads the official ECSV `*.csv.gz` bulk inventory
(one row per source). Each row exposes `source_id`, `solution_id`, `ra`, `dec`,
`flux`, and `flux_error`. The spectral columns are quoted CSV fields containing
bracketed comma-separated arrays with exactly 343 samples on the implicit XP
sampled grid 336–1020 nm (step 2 nm). NSB integrates only the inclusive
336–650 nm band (indices 0..=157). There is no per-row `wavelength` column in
the bulk product.

The tool streams each gzip file without loading the full inventory into memory,
fuses bulk checksum verification with the parse pass, and rejects the deprecated
long schema that assumed one CSV row per wavelength sample. The normalized
DataLink fallback (`--input`) still uses explicit semicolon-separated wavelength
series per source.

```bash
cargo run --locked -p nsb-data-tools --bin prepare_gaia_starlight_catalogue -- \
  --bulk-dir "$HOME/nsb-data/starlight-gaia-release/gaia_dr3_xp_sampled_bulk" \
  --output target/starlight-release/gaia_dr3_starlight_sources.csv \
  --diagnostics-output target/starlight-release/gaia_dr3_starlight_sources.diagnostics.json \
  --catalog-name "Gaia" \
  --catalog-release "DR3" \
  --catalog-license "$GAIA_DERIVED_PRODUCT_LICENSE_POLICY" \
  --photometry-model "gaia_dr3_xp_photon_radiance_336_650nm_v1" \
  --band-min-nm 336 \
  --band-max-nm 650 \
  --exclusions-output target/starlight-release/gaia_dr3_starlight_exclusions.csv
```

Scientific exclusions (non-positive passband integrals) are written to a
deterministic CSV sidecar with `source_id`, signed integral diagnostics, and
sample counts. Production runs require `--exclusions-output` when exclusions
exist. To regenerate only the sidecar from the official bulk inventory without
rewriting the canonical catalogue:

```bash
OUT="$HOME/nsb-data/starlight-gaia-release"
BULK="$OUT/gaia_dr3_xp_sampled_bulk"
LICENSE='Gaia DR3 data are open and free to use with credit to ESA/Gaia/DPAC; NSB redistributes only a derived validated runtime starlight map'

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

This command re-reads the bulk inventory, writes only the exclusions sidecar and
its diagnostics, and must yield exactly 10 scientific exclusions for the
validated 2026-07-11 run.

The validated canonical catalogue for the 2026-07-11 release run must not be
regenerated unnecessarily. Its streaming SHA-256 is:

```text
1ad31ac492cc85c9e7b777c96f905fc27290265f4d2d7d65870021a72217cf30
```

## Gaia DR3 nside sweep and reassessment

Candidate recommendation and production promotion are separate. A provisional
independent reference (`production_use: false` in
`validation/starlight_independent_reference_v1.json`) may still support
selecting the highest `nside` that passes internal science and operational
gates, but production remains blocked until reviewed external reference,
missing-flux assessment, and redistribution policy gates are satisfied.

Full sweep (rebuilds maps):

```bash
cargo run --locked --release -p nsb-data-tools --bin sweep_starlight_nside -- \
  --input "$HOME/nsb-data/starlight-gaia-release/gaia_dr3_starlight_sources.csv" \
  --output-dir "$HOME/nsb-data/starlight-gaia-release/sweep" \
  --reference validation/starlight_independent_reference_v1.json \
  --catalog-checksum "sha256:1ad31ac492cc85c9e7b777c96f905fc27290265f4d2d7d65870021a72217cf30" \
  --catalog-license "$GAIA_DERIVED_PRODUCT_LICENSE_POLICY" \
  --generation-date-utc "2026-07-11T14:02:43Z"
```

Reassess existing artefacts without rereading the 4.7 GiB catalogue or
rebuilding maps:

```bash
OUT="$HOME/nsb-data/starlight-gaia-release"
SWEEP="$OUT/sweep"
CATALOG_SHA="1ad31ac492cc85c9e7b777c96f905fc27290265f4d2d7d65870021a72217cf30"

cargo run --locked --release -p nsb-data-tools --bin sweep_starlight_nside -- \
  --output-dir "$SWEEP" \
  --assess-existing \
  --catalog-checksum "sha256:$CATALOG_SHA"
```

Observed 2026-07-11 reassessment:

```text
recommended_candidate_nside = 256
candidate_recommendation_passed = true
production_ready = false
production_blockers:
  independent_reference_not_approved_for_production
  missing_flux_report_not_approved
  redistribution_policy_not_approved
```

Use `--require-production-ready` only when an automated production promotion
is intended; the default candidate assessment exits successfully when a
candidate recommendation exists even if production is blocked.

The normalized DataLink fallback (`--input` to `prepare_gaia_starlight_catalogue`)
remains for controlled validation or repair when the official bulk inventory is
unavailable. See `docs/STELLAR_MAP_GENERATION.md` for the validated 336–650 nm
bulk workflow and candidate `nside=256` sweep outcome.
