# Build Starlight Map

This tool is the placeholder for generating `data/starlight_galactic_map_v1.csv`.

The standard NSB starlight model intentionally has no bundled fallback map until
a real catalogue-derived product is generated with recorded provenance. The
output CSV format expected by `components::starlight` is:

```csv
galactic_lon_deg,galactic_lat_deg,solid_angle_sr,integrated_ph_cm2_ns_sr,b_s10,v_s10
```

Required generator inputs before this can produce production data:

- source catalogue name, release, licence, and checksum
- magnitude limits and band definitions
- sky-pixelisation or lon/lat grid resolution
- conversion method from catalogue fluxes to integrated 300-650 nm photon
  radiance and B/V S10 values
- validation notebook or report against an independent reference

Until those inputs are available, `Starlight::standard_galactic_model()` returns
`NsbError::DataMissing`.
