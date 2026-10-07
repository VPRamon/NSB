# Stable CLI schemas

Status: Stable first-public-release schema contracts for the current CLI.
Audience: CLI consumers, downstream parsers, and maintainers.
Scope: JSON and CSV field contracts emitted by `nsb-cli`.
Non-goals: This document does not define scientific calibration requirements or
the Rust API.

NSB 0.1.0 establishes the initial public schema baseline. Schema identifiers
start at v1 and change only when fields are removed, renamed, retyped, or their
scientific meaning changes. Fields may be added compatibly to JSON.

## Point JSON v1

Identifier: `nsb-cli-point-json-v1`.

Top-level fields are `schema_version`, `version`, `model`, `time_utc`,
`observer`, `target`, `components`, `total`, and `band_diagnostic`.
Each component includes radiance, B/V diagnostics, relative uncertainty,
calibration status, provenance, validated domain, and band convention.
`version` includes NSB/model/Siderust versions and every runtime asset
checksum.

The `model` audit block records Airglow selection separately from Airglow
geometry and records the configured zodiacal model separately from propagation
metadata. Component provenance records the source and propagation actually
evaluated. Validated external Starlight provenance includes the source/map
checksums, licence/release, selection, photometry, generation command,
validation report, independent comparison, and calibration status.

## Window JSON v1

Identifier: `nsb-cli-window-json-v1`.

The output includes the same version/model audit block, selected component
metadata, requested bounds, and periods. Periods use RFC3339 UTC timestamps and
seconds.

## Point CSV v1

Identifier: `nsb-cli-point-csv-v1`.

Columns, in stable order, are:

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

The Airglow geometry columns are populated only on Airglow component rows and
are blank elsewhere. Van Rhijn rows carry implementation identity, effective
emission height, assumptions, provenance, and validated zenith domain.
Vertical-profile rows additionally carry the checksum-pinned profile identity,
schema, normalization, applicability, licence, and validated domain.

## Point CSV uncertainty v1

Identifier: `nsb-cli-point-csv-uncertainty-v1`.

When absolute component uncertainties are available, the point CSV contract
uses the same columns as Point CSV v1 and appends:

```text
statistical_uncertainty_ph_cm2_ns_sr,
systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr
```

A distinct identifier is used because the column layout differs.

## Window CSV v1

Identifier in every row: `nsb-cli-window-csv-v1`.

Window CSV inserts `record_type` after `schema_version`. Every result starts
with exactly one `query_summary` row followed by zero or more `period` rows.
A `query_summary` row records requested bounds, duration, selected components,
version fields, asset checksums, model preset, and applicable Airglow
geometry/profile provenance. A `period` row represents one matching interval
and repeats applicable model provenance for row-local auditability.

Consequently an empty result consists of the header plus its
`query_summary` row; it does not invent a matching period. Airglow fields are
populated only when Airglow was requested and remain blank for non-Airglow
queries.

The `asset_checksums` field is a semicolon-separated `path=sha256` list. CSV
quoting follows RFC 4180 through the Rust `csv` crate.
