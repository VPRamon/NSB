# nsb-data-tools

Offline data-generation tools for the NSB workspace.

These tools are not runtime dependencies of the `nsb` library crate and are not
part of the operational `nsb-cli` interface. They exist to build and validate
versioned scientific data products that can later be bundled by `crates/nsb`.

## `build_starlight_map`

Builds a Galactic HEALPix starlight map CSV compatible with
`nsb::components::starlight::StarlightMap`.

Output schema:

```csv
healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10
```

The data rows are preceded by metadata comments recording the coordinate frame,
HEALPix `nside`, ordering, source catalogue provenance, photometry model,
magnitude cuts, and generation timestamp.

Input schema, v1:

```csv
ra_deg,dec_deg,b_mag,v_mag[,weight,source_id]
```

- `ra_deg`, `dec_deg`: ICRS/J2000 equatorial coordinates in degrees.
- `b_mag`, `v_mag`: Johnson-like B/V magnitudes; blank values are treated as missing.
- `weight`: optional non-negative multiplicative weight, default `1`.
- `source_id`: optional source identifier preserved during catalogue parsing.

The tool:

1. Parses local catalogue rows into typed Siderust stellar catalogue records.
2. Delegates EquatorialMeanJ2000 → Galactic conversion, HEALPix binning, and
   starlight map construction to Siderust.
3. Converts B/V magnitudes to S10-like surface-brightness units using the
   approximation that `1 S10` is the flux of one 10th-magnitude star per square
   degree.
4. Writes every HEALPix pixel, including empty pixels, so the output is a complete
   full-sky map.
5. Records source catalogue provenance and map metadata as CSV comments.
6. Refuses to generate an all-zero map if no catalogue rows survive filters,
   unless `--allow-empty` is passed explicitly for tests/debugging.
7. Always hard-fails flux conservation; production catalogue builds should also
   pass `--require-science-diagnostics` so regional full-sky diagnostics fail CI
   instead of being reported as warnings.

Example:

```bash
cargo run -p nsb-data-tools --bin build_starlight_map -- \
  --input catalogue.csv \
  --output starlight_galactic_map_v1.csv \
  --nside 64 \
  --ordering ring \
  --max-v-mag 20 \
  --catalog-name "Example catalogue" \
  --catalog-release "v1" \
  --catalog-license "CC-BY-4.0" \
  --catalog-checksum "sha256:..." \
  --generation-date-utc "2026-06-21T00:00:00Z" \
  --require-science-diagnostics
```

The current integrated radiance is a transparent V-band proxy:

```text
integrated_ph_cm2_ns_sr = v_s10 * --integrated-per-v-s10
```

with default `--integrated-per-v-s10 = 1.242e-3`. Generated files record:

```text
# photometry_model=v_s10_scaled_integrated_v1
# band_definition=integrated 300-650 nm photon radiance plus B/V S10 diagnostics
```

Before using a generated file as the bundled production map, record and review:

- source catalogue name, release, license, and checksum;
- magnitude cuts and band definitions;
- HEALPix resolution and ordering;
- photometric conversion assumptions;
- validation against an independent reference.
