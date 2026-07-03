# Stable CLI schemas

Status: Stable v1 schema contract for the current CLI.
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
For validated external starlight, component provenance also carries source/map
checksums, licence/release, selection, photometry, generation command,
validation report, independent comparison, and calibration status. The external
checksum is component provenance rather than a bundled `data_assets` entry.

## Window JSON v1

Identifier: `nsb-cli-window-json-v1`.

The output includes the same version/model audit block, selected component
metadata, requested bounds, and periods. Periods use RFC3339 UTC timestamps and
seconds.

## Point CSV v1

Identifier in every row: `nsb-cli-point-csv-v1`.

Columns, in stable order:

```text
schema_version,record_type,component,integrated_ph_cm2_ns_sr,
b_s10_diagnostic,v_s10_diagnostic,b_mag_arcsec2_diagnostic,
v_mag_arcsec2_diagnostic,relative_uncertainty,calibration_status,
provenance,validated_domain,band_convention,nsb_version,model_version,
siderust_revision,model_preset,asset_checksums
```

## Window CSV v1

Identifier in every row: `nsb-cli-window-csv-v1`.

```text
schema_version,start_utc,end_utc,duration_seconds,components,nsb_version,
model_version,siderust_revision,model_preset,asset_checksums
```

The `asset_checksums` field is a semicolon-separated `path=sha256` list. CSV
quoting follows RFC 4180 through the Rust `csv` crate.
