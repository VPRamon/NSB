# Stable CLI schemas

Status: Stable, explicitly versioned schema contracts for the current CLI.
Audience: CLI consumers, downstream parsers, and maintainers.
Scope: JSON and CSV field contracts emitted by `nsb-cli`.
Non-goals: This document does not define scientific calibration requirements or
the Rust API.

Schema identifiers change only when fields are removed, renamed, retyped, or
their scientific meaning changes. Fields may be added compatibly to JSON.

## Point JSON v1

Identifier: `nsb-cli-point-json-v1`.

Top-level fields are `schema_version`, `version`, `model`, `time_utc`,
`observer`, `target`, `components`, `total`, and `band_diagnostic`. Each
component includes radiance, B/V diagnostics, relative uncertainty, calibration
status, provenance, validated domain, and band convention. `version` includes
NSB/model/Siderust versions and every runtime asset checksum.
Starlight component labels are stable by source: bundled production starlight is
`starlight`, a validated external override is `validated-starlight`, and a
caller-supplied experimental library map is labelled `experimental-starlight`.
For validated external starlight,
component provenance also carries source/map checksums, licence/release,
selection, photometry, generation command, validation report, independent
comparison, and calibration status. The external checksum is component
provenance rather than a bundled `data_assets` entry.

## Window JSON v1

Identifier: `nsb-cli-window-json-v1`.

The output includes the same version/model audit block, selected component
metadata, requested bounds, and periods. Periods use RFC3339 UTC timestamps and
seconds.

## Point CSV v1 and v2 (superseded)

The historical identifiers were `nsb-cli-point-csv-v1` for results without
absolute component uncertainties and `nsb-cli-point-csv-v2` when the three
absolute-uncertainty columns were present. These schemas did not serialize
structured Airglow geometry provenance and are no longer emitted.

Columns, in stable order:

```text
schema_version,record_type,component,integrated_ph_cm2_ns_sr,
b_s10_diagnostic,v_s10_diagnostic,b_mag_arcsec2_diagnostic,
v_mag_arcsec2_diagnostic,relative_uncertainty,calibration_status,
provenance,validated_domain,band_convention,nsb_version,model_version,
siderust_source,model_preset,asset_checksums
```

Point CSV v2 appended these columns:

```text
statistical_uncertainty_ph_cm2_ns_sr,
systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr
```

## Point CSV v3 and v4

Current identifiers are `nsb-cli-point-csv-v3` for results without absolute
component uncertainties and `nsb-cli-point-csv-v4` when the three absolute
uncertainty columns are present. Both schemas preserve structured Airglow
geometry identity. Columns, in stable order through the common portion, are:

```text
schema_version,record_type,component,integrated_ph_cm2_ns_sr,
b_s10_diagnostic,v_s10_diagnostic,b_mag_arcsec2_diagnostic,
v_mag_arcsec2_diagnostic,relative_uncertainty,calibration_status,
provenance,validated_domain,band_convention,nsb_version,model_version,
siderust_source,model_preset,asset_checksums,airglow_geometry_model,
airglow_geometry_version,airglow_geometry_emission_height_km,
airglow_profile_id,airglow_profile_schema_version,
airglow_profile_checksum_sha256,airglow_profile_normalization,
airglow_profile_altitude_min_km,airglow_profile_altitude_max_km,
airglow_profile_wavelength_min_nm,airglow_profile_wavelength_max_nm,
airglow_profile_wavelength_band,airglow_geometry_assumptions,
airglow_profile_validated_zenith_min_deg,
airglow_profile_validated_zenith_max_deg,airglow_geometry_provenance,
airglow_profile_license
```

Point CSV v4 appends:

```text
statistical_uncertainty_ph_cm2_ns_sr,
systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr
```

The Airglow geometry columns are populated only on Airglow component rows.
They are blank on other component rows and on the total row. Van Rhijn rows
carry the implementation identifier, effective emission height, assumptions,
provenance, and validated zenith domain. Vertical-profile rows additionally
carry the checksum-pinned profile identity, schema, normalization, altitude and
wavelength applicability, assumptions, provenance, licence, and validated
zenith domain.

## Window CSV v1 (superseded)

Identifier in every row: `nsb-cli-window-csv-v1`.

```text
schema_version,start_utc,end_utc,duration_seconds,components,nsb_version,
model_version,siderust_source,model_preset,asset_checksums
```

This historical schema did not serialize structured Airglow geometry
provenance and is no longer emitted.

## Window CSV v2 (superseded)

Identifier in every row: `nsb-cli-window-csv-v2`.

Window CSV v2 appends the same 17 Airglow geometry columns documented for
point CSV v3 to the window CSV v1 columns, in the same order. They are populated
from the selected Airglow component description when Airglow is requested and
remain blank when it is not. Consequently every emitted window period records
the exact configured Van Rhijn implementation or checksum-pinned vertical
profile that governed its Airglow evaluations.

This historical schema emitted only period rows. A valid query returning zero
periods therefore emitted only the header and could not preserve query-level
model provenance. It is no longer emitted.

## Window CSV v3

Identifier in every row: `nsb-cli-window-csv-v3`.

Window CSV v3 inserts `record_type` after `schema_version`; all remaining
columns retain the window CSV v2 order and meaning. Every result starts with
exactly one `query_summary` row followed by zero or more `period` rows:

- A `query_summary` row records the requested `start_utc`, `end_utc`, and
  `duration_seconds`, together with the selected components, version fields,
  asset checksums, model preset, and applicable Airglow geometry/profile
  provenance.
- A `period` row represents one matching interval. Its fields are populated as
  in window CSV v2, and its Airglow metadata repeats the query configuration for
  row-local auditability.

Consequently an empty result consists of the header plus its `query_summary`
row; it does not invent a matching period. Airglow fields are populated only
when Airglow was requested, including on empty results, and remain blank for
non-Airglow queries.

The `asset_checksums` field is a semicolon-separated `path=sha256` list. CSV
quoting follows RFC 4180 through the Rust `csv` crate.
