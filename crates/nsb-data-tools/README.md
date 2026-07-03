# nsb-data-tools

Offline, non-runtime tools for scientific data products.

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
- `validate_starlight_map`: emits a validation report for generated maps.
- `pack_starlight_asset`: frames a generated map and writes a checksum manifest
  for a derived bundled asset candidate.
- `verify_assets`: verifies the asset registry, required metadata, schemas,
  checksums, file coverage, and configured headers.

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

Production-style Gaia starlight generation must use
`gaia_dr3_xp_photon_radiance_330_650nm_v1`, pass `--require-science-diagnostics`,
and then pass the validation and packing stages. The legacy
`v_s10_scaled_integrated_proxy_v1` path remains experimental. See
`docs/STELLAR_MAP_GENERATION.md` for commands and promotion criteria.
