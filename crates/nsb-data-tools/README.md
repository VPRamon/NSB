# nsb-data-tools

Offline data-generation tools for the NSB workspace.

These tools are not runtime dependencies of the `nsb` library crate and are not
part of the operational `nsb-cli` interface. They exist to build and validate
versioned scientific data products that can later be bundled by `crates/nsb`.

## `build_starlight_map`

Builds a rectangular Galactic starlight map CSV compatible with
`nsb::components::starlight::StarlightMap`.

Output schema:

```csv
galactic_lon_deg,galactic_lat_deg,solid_angle_sr,integrated_ph_cm2_ns_sr,b_s10,v_s10
```

Input schema, v1:

```csv
ra_deg,dec_deg,b_mag,v_mag[,weight]
```

- `ra_deg`, `dec_deg`: ICRS/J2000 equatorial coordinates in degrees.
- `b_mag`, `v_mag`: Johnson-like B/V magnitudes.
- `weight`: optional non-negative multiplicative weight, default `1`.

The tool:

1. Converts equatorial coordinates to Galactic longitude/latitude using the
   standard J2000 Galactic rotation matrix.
2. Bins sources into a rectangular `(l, b)` grid.
3. Converts B/V magnitudes to S10-like surface-brightness units using the
   approximation that `1 S10` is the flux of one 10th-magnitude star per square
   degree.
4. Writes every pixel, including empty pixels, so the output is rectangular and
   accepted by `StarlightMap::from_csv_str`.
5. Records source catalogue provenance and calibration status as CSV comments.
6. Refuses to generate an all-zero map if no catalogue rows survive filters,
   unless `--allow-empty` is passed explicitly for tests/debugging.

Example:

```bash
cargo run -p nsb-data-tools --bin build_starlight_map -- \
  --input catalogue.csv \
  --output starlight_galactic_map_v1.csv \
  --lon-bin-deg 5 \
  --lat-bin-deg 5 \
  --max-v-mag 20 \
  --catalog-name "Example catalogue" \
  --catalog-release "v1" \
  --catalog-license "CC-BY-4.0" \
  --catalog-checksum "sha256:..."
```

The current integrated radiance is a transparent V-band proxy:

```text
integrated_ph_cm2_ns_sr = v_s10 * --integrated-per-v-s10
```

with default `--integrated-per-v-s10 = 1.242e-3`. Generated files are marked as:

```text
# calibration_status=proxy_not_production
# photometry_model=v_s10_scaled_integrated_proxy
```

Before using a generated file as the bundled production map, record and review:

- source catalogue name, release, licence, and checksum;
- magnitude cuts and band definitions;
- sky-grid resolution;
- photometric conversion assumptions;
- validation against an independent reference.
