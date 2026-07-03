# Starlight map validation

`validate_starlight_map` is the release harness for generated starlight maps:

```bash
cargo run --locked -p nsb-data-tools --bin validate_starlight_map -- \
  --input target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.csv \
  --diagnostics target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.diagnostics.json \
  --reference "$STARLIGHT_INDEPENDENT_VALIDATION_REFERENCE" \
  --output target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.validation.json \
  --require-independent-comparison
```

The harness verifies parseability, finite/nonnegative radiance fields, pixel
count, integrated plane/pole contrast, and records whether independent
comparison evidence is available. Production calibration is not claimed unless
`production_ready=true` in the validation report, which requires independent
comparison evidence outside the tiny CI fixtures.

Without that evidence, `pack_starlight_asset --candidate` can produce a clearly
labelled review artifact, but `--production` fails. NSB must not promote such an
artifact into `ComponentMask::ALL` or label it as bundled production starlight.
