# Observatory configuration and customisation

Status: Current configuration guide.
Audience: Observatory users, integrators, developers, and maintainers.
Scope: Coordinates, built-in profiles, runtime model options, custom data, and the path to a calibrated observatory profile.

NSB separates **where the observer is** from **which atmospheric and scientific
assumptions are used**.

| Concern | Owner | Selected by |
| --- | --- | --- |
| Observatory location | Siderust `Observatory` / `ObservatoryCatalog` | `--site`, `--observatory-catalog`, or `--lon/--lat/--height` |
| NSB scientific assumptions | NSB `SiteProfile` | `--site-profile` (default `generic-clear-sky`) |

Supplying observatory coordinates is straightforward; claiming a site-calibrated
model requires explicit data, provenance, validation, and code-level admission.
Inclusion in a location catalog does **not** imply a calibrated NSB profile.

## Catalog layers and precedence

The CLI composes three layers:

1. **Siderust bundled observatories** — generic facilities shipped with
   `ObservatoryCatalog::builtin()` (Paranal, Roque de los Muchachos, Mauna Kea,
   La Silla, …).
2. **NSB bundled extensions** — `crates/nsb-cli/data/observatories.toml` using
   the same Siderust `[[observatory]]` schema. This is where CTAO-N/S and other
   NSB-relevant facilities live without requiring them upstream in Siderust.
3. **User-provided catalog** — `--observatory-catalog path/to/file.toml`.

Deterministic policy:

- Without `--observatory-catalog`, the effective catalog is Siderust builtins
  **extended** with NSB's bundled TOML. Exact name collisions between those
  layers are hard errors; there is no silent override.
- With `--observatory-catalog`, the user file **replaces** the entire effective
  catalog for that command (neither Siderust builtins nor NSB extensions are
  consulted).
- Duplicate exact names inside one TOML file are rejected by Siderust.
- CTAO-N is never an alias for ORM; CTAO-S is never an alias for Paranal.

NSB CLI aliases in `crates/nsb-cli/data/observatory-aliases.toml` map short
names onto exact catalog names only. They carry neither coordinates nor
`SiteProfile` selection.

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

Inspect the effective catalog with:

```bash
nsb sites list
nsb sites show PARANAL
nsb sites show CTAO-N
nsb sites show CTAO-S
nsb sites show HESS
```

Observatory selection never selects a scientific profile. For example:

```bash
nsb point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-N \
  --site-profile cta-north \
  --ra 83.6331 \
  --dec 22.0145
```

`--site CTAO-N` alone still uses `generic-clear-sky`. The CTAO planning profiles
carry machine-readable provenance and assumptions, but are not site-calibrated.
CTAO North/South are distinct physical array locations from ORM/Paranal.

Location and scientific assumptions are deliberately orthogonal. For example,
`--site PARANAL --site-profile cta-south` is valid when intentionally evaluating
CTAO-S planning assumptions at Paranal geometry. It is a decoupling example,
not a claim that Paranal and CTAO-S are the same observatory.

### External observatory catalogs

Use Siderust's native catalog format:

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

An external catalog replaces the effective lookup and listing scope for that
command. NSB aliases are applied only when their target name exists in the
active catalog. Custom observatories default to `generic-clear-sky` unless
`--site-profile` is set.

Rust applications can select CTAO planning assumptions directly:

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
  --site CTAO-S \
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

## Level 5: extend the NSB location catalog or add an alias

To expose a new facility in the default CLI catalog without waiting for a
Siderust release, add an `[[observatory]]` record to
`crates/nsb-cli/data/observatories.toml` using Siderust's schema. Do not
redefine an exact Siderust builtin name.

To add only a short name for an existing catalog record, add an entry to
`crates/nsb-cli/data/observatory-aliases.toml`. Aliases do not map to
`SiteProfileId`; profile selection remains explicit.

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
- The built-in CTAO profiles are planning presets, not calibrated products.
- Site-specific airglow or atmospheric parameters are not currently exposed as
  arbitrary CLI flags; a validated new profile is a library and data change.
