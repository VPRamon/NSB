# Validated external starlight manifest

Status: Current fail-closed sidecar contract.
Audience: Integrators supplying external starlight maps and maintainers reviewing
their evidence.
Scope: Manifest fields, runtime admission checks, and the limits of external
production metadata.
Non-goals: This document does not validate any specific external catalogue or
license.

NSB uses this sidecar contract for caller-provided production starlight and for
the bundled Gaia DR3 release manifest. CLI `--components starlight` uses the
bundled production asset when one is registered; `--starlight-map` plus
`--starlight-manifest` provide a validated external override. The library
equivalent for external files is `ValidatedStarlightMap::from_files`.

The manifest uses schema version 1 and denies unknown fields. All string fields
below are mandatory and non-empty. `calibration_status` must be `production`;
`photometry_model` must not contain `proxy` or `experimental`; and
`flux_conservation_validated` must be true. `input_b_flux_sum`,
`input_v_flux_sum`, and `flux_conservation_tolerance` are optional only as a
group. When present, NSB recomputes the conservation check.

```toml
schema_version = 1
calibration_status = "production"
dataset_name = "<derived product name>"
version = "<immutable release>"
generation_date = "<RFC3339 UTC>"
source_catalogue = "<catalogue name>"
source_catalogue_release = "<release>"
source_catalogue_license = "<reviewed license>"
source_catalogue_checksum = "sha256:<source checksum>"
source_selection = "<filters, quality cuts, and completeness>"
magnitude_limit = "<bright/faint limits>"
map_resolution = "HEALPix nside=<N> ordering=<ring|nested>"
photometry_model = "<validated non-proxy model identifier>"
band_definition = "<integrated passband and units>"
smoothing = "<kernel/FWHM or none>"
generated_by = "<tool and version>"
generation_command = "<reproducible command>"
map_sha256 = "sha256:<exact map byte checksum>"
validation_report = "<reviewable report path or identifier>"
independent_comparison = "<published/trusted reference and result>"
flux_conservation_validated = true

# Optional, but all three must appear together.
input_b_flux_sum = 0.0
input_v_flux_sum = 0.0
flux_conservation_tolerance = 1e-9

[header]
# Exact key/value contract copied from the map comments.
map_type = "healpix"
coordinate_frame = "galactic"
nside = "<N>"
ordering = "ring"
dataset_name = "<same as above>"
version = "<same as above>"
generation_date_utc = "<same as generation_date>"
source_catalogue = "<same as above>"
source_catalogue_release = "<same as above>"
source_catalogue_license = "<same as above>"
source_catalogue_checksum = "<same as above>"
source_selection = "<same as above>"
magnitude_limit = "<same as above>"
map_resolution = "<same as above>"
calibration_status = "production"
photometry_model = "<same as above>"
band_definition = "<same as above>"
smoothing = "<same as above>"
generated_by = "<same as above>"
generation_command = "<same as above>"
validation_report = "<same as above>"
independent_comparison = "<same as above>"
```

The header table may include additional exact-match keys. The map checksum is
not embedded in the map because that would be self-referential.

Admission verifies complete Galactic HEALPix coverage, finite/nonnegative
values, plane/pole V-S10 ratio of at least 1, and longitude-seam relative jump
of at most 1. These construction diagnostics do not replace review of the
external calibration and license evidence.
