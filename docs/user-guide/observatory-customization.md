# Observatory configuration and customisation

Status: Current configuration guide.
Audience: Observatory users, integrators, developers, and maintainers.
Scope: Coordinates, built-in profiles, runtime model options, custom data, and the path to a calibrated observatory profile.

NSB separates **where the observer is** from **which atmospheric and scientific
assumptions are used**. Supplying observatory coordinates is straightforward;
claiming a site-calibrated model requires explicit data, provenance, validation,
and code-level admission.

## Level 1: use arbitrary observatory coordinates

The CLI accepts a named alias or an explicit geodetic position.

```bash
nsb point \
  --time 2026-06-18T23:00:00Z \
  --lon -70.316344 \
  --lat -24.683428 \
  --height 2150 \
  --ra 83.6331 \
  --dec 22.0145
```

For explicit coordinates, the runtime uses the generic clear-sky profile. The
observer height is used to derive generic atmospheric pressure, while the sky
geometry is evaluated at the supplied longitude and latitude.

This level is appropriate when you need correct observatory geometry but do not
have a validated site-specific atmospheric or airglow calibration.

## Level 2: use a bundled named observatory

The CLI ships an NSB-owned catalog at
`crates/nsb-cli/data/observatories.toml`. It includes CTAO-S, CTAO-N, H.E.S.S.,
MAGIC, FACT, VERITAS, FAST, GTC, Paranal, Roque de los Muchachos, Mauna Kea,
and La Silla. Inspect the current catalog with:

```bash
nsb sites list
nsb sites show CTAO-S
nsb sites show HESS
```

The bundled catalog is a location catalog, not a calibration catalog. It is kept
in NSB so adding a facility useful to NSB does not require adding that facility
to Siderust or waiting for a Siderust release. Siderust remains the provider of
coordinate and quantity types used by the runtime; it is not the authority for
which named sites NSB exposes.

CTAO-S and CTAO-N currently select explicit NSB planning profiles. These
profiles carry machine-readable provenance and assumptions, but they are not
marked as site-calibrated. Other bundled sites use the generic clear-sky profile
unless the application configures another supported profile explicitly.

Rust applications can select the CTAO planning assumptions directly:

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
  --site CTAO-S \
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
  --site CTAO-S \
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

## Level 5: add a new bundled named observatory

A CLI site is an operational location, not a scientific calibration. To add one,
maintainers add a `[[site]]` record to
`crates/nsb-cli/data/observatories.toml` with:

- a stable canonical alias;
- display name;
- east-positive longitude;
- north-positive latitude;
- representative site/telescope altitude in metres;
- accepted alternative aliases;
- one or more HTTPS provenance sources.

The catalog loader validates finite coordinate ranges, non-empty source
provenance, canonical aliases, and globally unique normalized aliases. Add a CLI
contract test for important new facilities. Do not silently map a new observatory
to CTAO-N or CTAO-S merely because its altitude or climate is similar.

Adding a named location here does **not** require adding it to Siderust's bundled
catalog. This keeps NSB free to support facilities such as H.E.S.S., FAST, or a
future experiment without an upstream data release.

## Level 6: add a calibrated observatory profile

A calibrated profile is a scientific product. It requires changes in the
runtime library and reproducible validation evidence.

1. Add a stable `SiteProfileId` variant in `crates/nsb/src/site.rs`.
2. Define explicit atmospheric and airglow assumptions with provenance.
3. Add a constructor or configuration path in `NsbModelConfig`.
4. Map CLI aliases to the new profile when appropriate.
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
- Arbitrary CLI coordinates and bundled sites other than CTAO-N/S use generic
  clear-sky assumptions unless a supported profile is configured separately.
- The built-in CTAO profiles are planning presets, not calibrated products.
- Site-specific airglow or atmospheric parameters are not currently exposed as
  arbitrary CLI flags; a validated new profile is a library and data change.
