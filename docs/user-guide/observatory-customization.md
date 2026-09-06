# Observatory configuration and customisation

Status: Current configuration guide.
Audience: Observatory users, integrators, developers, and maintainers.
Scope: Coordinates, built-in profiles, runtime model options, custom data, and the path to a calibrated observatory profile.

NSB separates **where the observer is** from **which atmospheric and scientific
assumptions are used**. Siderust owns observatory identity, canonical WGS84
coordinates, reference atmosphere, and catalog parsing. NSB `SiteProfile`
represents NSB-specific scientific assumptions and is selected independently.
Supplying observatory coordinates is straightforward;
claiming a site-calibrated model requires explicit data, provenance, validation,
and code-level admission.

## Level 1: use arbitrary observatory coordinates

The CLI accepts a named alias or an explicit geodetic position.

```bash
nsb point \
  --time 2026-06-18T23:00:00Z \
  --lon 12.5 \
  --lat 41.9 \
  --height 800 \
  --ra 83.6331 \
  --dec 22.0145
```

Unless `--site-profile` is supplied, the runtime uses the generic clear-sky profile. The
observer height is used to derive generic atmospheric pressure, while the sky
geometry is evaluated at the supplied longitude and latitude.

This level is appropriate when you need correct observatory geometry but do not
have a validated site-specific atmospheric or airglow calibration.

## Level 2: use a named observatory and choose a profile

Named observatories come from Siderust's bundled `ObservatoryCatalog`. NSB adds
only CLI aliases such as `PARANAL`, `VLT`, `ORM`, `LA-PALMA`, `MAUNA-KEA`, and
`LA-SILLA`; aliases contain no coordinates or scientific properties. Inspect
the active catalog with:

```bash
nsb sites list
nsb sites show PARANAL
```

Observatory selection never selects a scientific profile. For example,
`--site PARANAL --site-profile cta-south` explicitly combines the Siderust
Paranal location with NSB's CTAO-South planning assumptions. The CTAO profiles
carry machine-readable provenance and assumptions, but are not site-calibrated.
CTAO-North and CTAO-South are not aliases for ORM and Paranal; until Siderust
ships distinct records, their location aliases intentionally do not resolve.

### External observatory catalogs

Use Siderust's native catalog format to select an observatory unknown when NSB
was compiled:

```toml
[[observatory]]
name = "My Observatory"
longitude_deg = 12.5
latitude_deg = 41.9
height_m = 800.0
reference_pressure_hpa = 920.0
reference_temperature_k = 283.0
reference_relative_humidity = 0.4
```

```bash
nsb point \
  --time 2026-06-18T23:00:00Z \
  --site "My Observatory" \
  --observatory-catalog observatories.toml \
  --ra 83.6331 \
  --dec 22.0145
```

An external catalog replaces the bundled lookup and listing scope for that
command. This makes duplicate behavior unambiguous; duplicate exact names
inside the external file are rejected by Siderust. NSB aliases are applied only
when their target name exists in the active catalog.

Rust applications can select them directly:

```rust,no_run
use nsb::{NsbEvaluator, NsbModelConfig};

# fn build() -> nsb::Result<()> {
let south = NsbEvaluator::with_config(NsbModelConfig::cta_s_planning())?;
let north = NsbEvaluator::with_config(NsbModelConfig::cta_n_planning())?;
# let _ = (south, north);
# Ok(())
# }
```

See [CTAO site-profile assumptions](../specifications/ctao-site-profiles.md) for the exact
pressure, aerosol, and airglow limitations.

## Level 3: adjust supported runtime parameters

The CLI exposes model choices that are safe to vary per run:

```bash
nsb point \
  --time 2026-06-18T23:00:00Z \
  --site PARANAL \
  --site-profile cta-south \
  --ra 83.6331 \
  --dec 22.0145 \
  --solar-radio-flux-sfu 130 \
  --moonlight-model jones2013 \
  --zodiacal-extinction noll2012
```

Supported adjustments include:

- component selection;
- airglow F10.7 solar radio flux;
- moonlight implementation;
- zodiacal extinction choice;
- threshold-search constraints and sampling step;
- output format and logging level.

These options change a model input or implementation choice. They do not create
a new calibrated observatory profile.

## Level 4: supply a validated external starlight product

An observatory or experiment may provide its own integrated starlight map:

```bash
nsb --format json point \
  --time 2026-06-18T23:00:00Z \
  --site PARANAL \
  --site-profile cta-south \
  --ra 83.6331 \
  --dec 22.0145 \
  --components starlight \
  --starlight-map /data/observatory/starlight.csv \
  --starlight-manifest /data/observatory/starlight.toml
```

Both files are required. Runtime admission verifies the map checksum, exact
header contract, complete HEALPix coverage, finite non-negative values,
provenance, validation references, calibration status, and other production
gates. Failure is fatal; there is no bundled experimental starlight fallback.

The complete sidecar schema is documented in
[Validated external starlight manifest](../nsb_components/starlight/external-manifest.md).

## Level 5: add a new named observatory alias

A CLI alias is an operational convenience, not a scientific calibration. Add
only an `alias = "Exact Siderust observatory name"` entry to
`crates/nsb-cli/data/observatory-aliases.toml`. Canonical coordinates and
reference conditions belong in Siderust, never in this alias file. Aliases do
not map to `SiteProfileId`; profile selection remains explicit.

## Level 6: add a calibrated observatory profile

A calibrated profile is a scientific product. It requires changes in the
runtime library and reproducible validation evidence.

1. Add a stable `SiteProfileId` variant in `crates/nsb/src/site.rs`.
2. Define explicit atmospheric and airglow assumptions with provenance.
3. Add a constructor or configuration path in `NsbModelConfig`.
4. Expose explicit profile selection where appropriate.
5. Register every new immutable asset in `crates/nsb/data/manifest.toml` with a
   checksum, license, source, and maturity.
6. Add unit, regression, metadata, and end-to-end validation tests.
7. Update model maturity, validation, and observatory documentation.
8. Promote the profile to `Calibrated` only after reproducible site-reference
   comparisons pass the documented tolerances.

The minimum evidence should cover pressure and altitude, Rayleigh assumptions,
aerosol/Mie parameters, airglow continuum and temporal corrections, and the
resulting effect on moonlight and airglow predictions.

## Generating or updating scientific data

Runtime assets must not be edited manually without updating their manifest and
validation evidence. Use `nsb-data-tools` for acquisition, transformation,
validation, and packaging. Start with the
[maintainer dataset workflow](../maintainer-guide/datasets.md).

## Current limitations

- The CLI can generate and validate a TOML configuration template, but point and
  window commands do not yet execute directly from a `--config` file.
- Arbitrary coordinates and all named/custom observatories use generic
  clear-sky assumptions unless `--site-profile` is selected explicitly.
- Distinct CTAO-North/South locations are pending Siderust issue
  [#93](https://github.com/Siderust/siderust/issues/93); NSB does not substitute
  ORM or Paranal.
- The built-in CTAO profiles are planning presets, not calibrated products.
- Site-specific airglow or atmospheric parameters are not currently exposed as
  arbitrary CLI flags; a validated new profile is a library and data change.
