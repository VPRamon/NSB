# Starlight map validation

`validate_starlight_map` is the release harness for generated starlight maps:

```bash
cargo run --locked -p nsb-data-tools --bin validate_starlight_map -- \
  --input target/starlight-release/starlight_galactic_map_v1.csv \
  --diagnostics target/starlight-release/starlight_galactic_map_v1.diagnostics.json \
  --output target/starlight-release/starlight_galactic_map_v1.validation.json
```

The harness verifies parseability, finite/nonnegative radiance fields, pixel
count, integrated plane/pole contrast, and records whether independent
comparison evidence is available. Production calibration is not claimed unless
`production_ready=true` in the validation report, which requires independent
comparison evidence outside the tiny CI fixtures.

Without that evidence, `pack_starlight_asset` can produce a candidate derived
asset, but NSB must not promote it into `ComponentMask::ALL` or label it as
bundled production starlight.
