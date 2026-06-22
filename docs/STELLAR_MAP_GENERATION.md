# Stellar map generation

NSB models integrated starlight as a direction-dependent sky map rather than a runtime scalar formula. The stellar background is concentrated around the Galactic plane, varies strongly with Galactic longitude, and depends on catalogue depth, photometry, masking, and map resolution. For that reason the runtime library consumes a bundled map, while the expensive catalogue preparation and map construction live in `crates/nsb-data-tools`.

## Status of the bundled v1 map

`crates/nsb/data/starlight_galactic_map_v1.csv` is bundled so that `StarlightModel::BundledCatalogueMap` and `ComponentMask::ALL` can evaluate without runtime downloads or local data paths. The current asset is explicitly labelled `Experimental` in the CSV header. It is not a production CTAO calibration and must not be cited as an externally validated sky-brightness product.

Promotion beyond `Experimental` requires a regenerated catalogue-derived product with reviewed source-catalogue provenance, checksums, validation diagnostics, and comparison against independent references or site measurements.

## Data flow

The intended offline pipeline is:

```text
local reviewed stellar catalogue
  -> prepare_tycho_starlight_catalogue
  -> canonical catalogue CSV
  -> build_starlight_map
  -> crates/nsb/data/starlight_galactic_map_v1.csv
  -> nsb::StarlightModel::BundledCatalogueMap
```

Runtime code must not download Gaia, Tycho, Hipparcos, or other external catalogues. The bundled CSV is loaded at compile time with `include_str!`, so a missing asset fails during build rather than at runtime through `CARGO_MANIFEST_DIR` path lookup.

## Canonical catalogue schema

`build_starlight_map` consumes:

```csv
ra_deg,dec_deg,b_mag,v_mag,weight,source_id
```

- `ra_deg`, `dec_deg`: ICRS/J2000 equatorial coordinates in degrees.
- `b_mag`, `v_mag`: Johnson-like B/V magnitudes; blank values are accepted where supported by the builder.
- `weight`: non-negative multiplicative source weight. Omitted values default to `1` in preparation tools.
- `source_id`: optional source identifier for traceability.

## Tycho preparation tool

`prepare_tycho_starlight_catalogue` converts a local Tycho/Hipparcos-style CSV extract into the canonical schema:

```bash
cargo run -p nsb-data-tools --bin prepare_tycho_starlight_catalogue -- \
  --input tycho_extract.csv \
  --output catalogue_for_starlight.csv \
  --diagnostics-output catalogue_for_starlight.diagnostics.txt \
  --catalog-name "Tycho-2" \
  --catalog-release "2000" \
  --catalog-license "REVIEW-ME" \
  --input-checksum "sha256:REPLACE_ME" \
  --max-v-mag 11.5
```

The preparation tool expects local input columns:

```csv
ra_deg,dec_deg,bt_mag,vt_mag,weight,source_id
```

`weight` and `source_id` are optional. The BT/VT to Johnson-like B/V transform is an approximate proxy labelled `tycho_bt_vt_to_johnson_bv_proxy_v1`. It is suitable for an experimental first pipeline but not for production passband calibration.

## Map generation

`build_starlight_map` remains the main map builder:

```bash
cargo run -p nsb-data-tools --bin build_starlight_map -- \
  --input catalogue_for_starlight.csv \
  --output crates/nsb/data/starlight_galactic_map_v1.csv \
  --nside 64 \
  --ordering ring \
  --max-v-mag 11.5 \
  --catalog-name "Tycho-2" \
  --catalog-release "2000" \
  --catalog-license "REVIEW-ME" \
  --catalog-checksum "sha256:REPLACE_ME" \
  --generation-date-utc "REPLACE_WITH_ACTUAL_UTC_TIMESTAMP" \
  --require-science-diagnostics
```

The builder is an orchestration layer. Siderust owns the generic HEALPix grid, EquatorialMeanJ2000-to-Galactic conversion, stellar surface-brightness map construction, flux-conservation validation, plane/pole checks, and longitude-wrap diagnostics.

## Output schema and metadata

The bundled map is a Galactic HEALPix CSV:

```csv
# map_type=healpix
# nside=64
# ordering=ring
# coordinate_frame=galactic
# dataset_name=...
# version=v1
# generation_date_utc=...
# source_catalogue=...
# source_catalogue_release=...
# source_catalogue_license=...
# source_catalogue_checksum=...
# magnitude_limit=...
# calibration_status=Experimental
# photometry_model=v_s10_scaled_integrated_v1
# band_definition=integrated 300-650 nm photon radiance plus B/V S10 diagnostics
# generated_by=nsb-data-tools build_starlight_map using siderust
healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10
```

The runtime loader parses these header values into `StarlightProvenance`, and component metadata reports the map as `Experimental`.

## Validation expectations

The generation pipeline should verify:

- every expected HEALPix pixel is present;
- all values are finite;
- all integrated radiance and S10 values are non-negative;
- source flux is conserved within the configured tolerance;
- Galactic-plane brightness exceeds Galactic-pole brightness for production-style builds;
- longitude wrapping around `l = 0 / 360 deg` does not introduce an empty or discontinuous seam;
- provenance fields are complete enough to reproduce the product.

`--require-science-diagnostics` should be used for any candidate bundled release asset. If diagnostics fail, keep the result out of production workflows and leave the metadata status as `Experimental`.

## Limitations of v1

The current v1 photometry model uses a transparent V-S10-scaled integrated-radiance proxy:

```text
integrated_ph_cm2_ns_sr = v_s10 * integrated_per_v_s10
```

This is not full spectral/passband synthesis. Future work should replace it with passband-aware integration and validate the resulting map against SkyCalc-style references, published dark-sky measurements, or site-specific observations.
