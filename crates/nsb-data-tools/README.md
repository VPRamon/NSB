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
- `prepare_gaia_starlight_catalogue`: converts a maintainer Gaia DR3 + XP
  release extract into canonical passband-integrated source rows using Siderust
  Gaia/passband APIs.
- `build_starlight_map`: delegates transforms, HEALPix, construction, and
  validators to Siderust; writes a complete map and optional JSON diagnostics.
  It supports both the legacy proxy B/V input and Gaia passband photon-flux
  source tables.
- `sweep_starlight_nside`: runs candidate map builds for multiple HEALPix
  resolutions and writes a summary used to choose the final bundled resolution.
- `validate_starlight_map`: emits a validation report for generated maps.
- `pack_starlight_asset`: frames a generated map and writes a checksum manifest
  for a derived bundled asset candidate or production asset.
- `verify_assets`: verifies the asset registry, required metadata, schemas,
  checksums, file coverage, and configured headers.

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

## Gaia DR3 release input generation

Smoke/candidate run, suitable for checking local plumbing before a full Gaia
release extraction:

```bash
cargo run --locked -p nsb-data-tools --bin generate_gaia_starlight_release_inputs -- \
  --out-dir target/starlight-smoke \
  --max-g-mag 12.0 \
  --limit 1000 \
  --chunk-size 100 \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --candidate \
  --resume
```

Production-style run, once reviewed policy and independent validation files
exist:

```bash
cargo run --locked -p nsb-data-tools --bin generate_gaia_starlight_release_inputs -- \
  --out-dir target/starlight-release \
  --max-g-mag 20.0 \
  --chunk-size 5000 \
  --band-min-nm 330 \
  --band-max-nm 650 \
  --license-policy-file docs/GAIA_DERIVED_PRODUCT_POLICY.md \
  --validation-reference validation/starlight_independent_reference_v1.json \
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

After generation, source the env file and run the remaining pipeline:

```bash
source target/starlight-release/starlight_release_inputs.env

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
```

Production-style Gaia starlight generation must use
`gaia_dr3_xp_photon_radiance_330_650nm_v1`, pass `--require-science-diagnostics`,
and then pass the validation and packing stages. The legacy
`v_s10_scaled_integrated_proxy_v1` path remains experimental. See
`docs/STELLAR_MAP_GENERATION.md` for commands and promotion criteria.
