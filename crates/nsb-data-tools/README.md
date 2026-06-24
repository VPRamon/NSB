# nsb-data-tools

Offline, non-runtime tools for scientific data products.

- `prepare_tycho_starlight_catalogue`: converts local BT/VT catalogue rows to
  canonical rows, verifies input SHA-256, and emits JSON diagnostics. Its colour
  transform is explicitly experimental.
- `build_starlight_map`: delegates transforms, HEALPix, construction, and
  validators to Siderust; writes a complete map and optional JSON diagnostics.
- `verify_assets`: verifies the asset registry, required metadata, schemas,
  checksums, file coverage, and configured headers.

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

Production-style starlight generation must pass
`--require-science-diagnostics`, which also requires catalogue release, license,
and checksum metadata. The current integrated conversion is recorded as
`v_s10_scaled_integrated_proxy_v1`; generated products remain experimental until
passband-aware and independent validation gates pass. See
`docs/STELLAR_MAP_GENERATION.md` for commands and promotion criteria.
