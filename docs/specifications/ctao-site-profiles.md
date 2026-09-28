# CTAO site-profile assumptions

Status: Current planning-preset contract (application / CLI layer).
Audience: CTAO planning users, reviewers, and maintainers.
Scope: CTAO North/South planning-profile assumptions, exposed metadata, and
promotion requirements.
Non-goals: This document does not claim site calibration for CTAO-N or CTAO-S.
The generic `nsb` library does not own CTAO presets in its public API.

NSB distinguishes generic clear-sky fallbacks from named scientific profiles
through typed `SiteProfile<P>` values, `SiteProfileTag` markers, and
`CalibrationStatus`. Compile-time marker types carry identity; `NAME` strings are
metadata only. Profile resolution is an internal concern. Observatory- or
project-named planning presets such as CTAO North/South define local marker types
at the application layer (for example `nsb-cli`) and supply typed profiles to
`NsbModelConfig::with_site_profile`, which erases the marker internally.

A site-profile marker is not an observatory identifier. Siderust
observatory/catalog selection answers **where the observer is**; a
`SiteProfile<P>` answers **which NSB scientific assumptions and evidence-backed
maturity are used**.
Selecting `--site CTAO-N`, `--site CTAO-S`, `--site PARANAL`, ORM, or arbitrary
coordinates does not select a scientific profile. The CLI defaults to
`generic-clear-sky` unless `--site-profile` is supplied explicitly.

The current CTAO entries are **planning presets**, not fully site-calibrated
science products. They exist so CTAO call sites can select explicit assumptions
and inspect provenance instead of implicitly using generic assumptions. Their
maturity remains `PlanningPreset` until the scientific gate in issue #38 is
completed.

## Profiles

| Profile | Layer | Status | Atmosphere | Airglow |
| --- | --- | --- | --- | --- |
| `SiteProfile::<GenericClearSky>::generic_clear_sky()` / `NAME` `generic-clear-sky` | core `nsb` | `GenericFallback` | Pressure derived from observer altitude; default Rayleigh scale height; bundled clear-sky Mie parameters. | Bundled Paranal-derived `NSB/data/airglow_cont.dat` continuum with neutral scale; generic/planning proxy even at Paranal. |
| CLI `--site-profile cta-north` / marker `CtaNorth`, `NAME` `ctao-north-planning` | `nsb-cli` | `PlanningPreset` | Representative La Palma/ORM-like planning altitude/pressure assumptions, default Rayleigh scale height, Paranal-like bundled Mie parameters. These are scientific assumptions, not CTAO-N/ORM location aliases. | Bundled Paranal-derived continuum with neutral scale; no CTA-N-specific continuum calibration is bundled yet. |
| CLI `--site-profile cta-south` / marker `CtaSouth`, `NAME` `ctao-south-planning` | `nsb-cli` | `PlanningPreset` | Paranal-like `AtmosphereProfile::EL_PARANAL` planning assumptions. This does not identify the observer as Paranal. | Bundled Paranal-derived continuum with neutral scale; no CTA-S-specific continuum calibration is bundled yet. |

Paranal provenance records the empirical lineage of the bundled continuum. It is
not evidence that the generic model is automatically calibrated at Paranal or
that CTAO-S inherits a Paranal calibration.

## API usage

Application layer (`nsb-cli`): CTAO presets are marker types in
`crates/nsb-cli/src/site_profiles.rs`:

```rust
use nsb::site::{AtmosphericConditions, SiteProfile, SiteProfileTag};
use nsb::{CalibrationStatus, NsbEvaluator, NsbModelConfig};
use siderust::qtty::Kilometers;

// Defined in nsb-cli, not in crates/nsb:
// struct CtaSouth; impl SiteProfileTag for CtaSouth { const NAME = "ctao-south-planning"; }

let ctao_south = SiteProfile::<CtaSouth>::planning(
    Kilometers::new(2.1),
    AtmosphericConditions::paranal_average(),
    "CTAO-South planning preset: Paranal-like atmosphere ...",
)?;
let config = NsbModelConfig::generic_clear_sky().with_site_profile(ctao_south);
assert_eq!(config.site_profile_name(), "ctao-south-planning");
assert_eq!(
    config.airglow_calibration_status(),
    CalibrationStatus::PlanningPreset,
);
assert!(!config.is_airglow_site_calibrated());
let evaluator = NsbEvaluator::with_config(config)?;
```

External integrators define their own markers without changing `crates/nsb`:

```rust
use nsb::site::{AtmosphericConditions, SiteProfile, SiteProfileTag};
use nsb::NsbModelConfig;
use siderust::qtty::{Hectopascals, Kilometers};

struct MagicLaPalma;

impl SiteProfileTag for MagicLaPalma {
    const NAME: &'static str = "magic-la-palma-planning";
}

let magic = SiteProfile::<MagicLaPalma>::planning(
    Kilometers::new(2.2),
    AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0))?,
    "Caller-defined MAGIC planning atmosphere.",
)?;
let config = NsbModelConfig::generic_clear_sky().with_site_profile(magic);
```

Moonlight scientific-model selection remains independent of the site profile:

```rust
use nsb::{MoonlightModel, NsbEvaluator, NsbModelConfig};

let config = NsbModelConfig::default()
    .with_site_profile(ctao_south)
    .with_moonlight_model(MoonlightModel::Jones2013Spectral);

assert_eq!(config.site_profile_name(), "ctao-south-planning");
assert_eq!(
    config.moonlight_model(),
    MoonlightModel::Jones2013Spectral,
);
let evaluator = NsbEvaluator::with_config(config)?;
```

Concrete Moonlight evaluator types are internal; normal evaluation goes through
`NsbEvaluator`. Jones consumes the selected profile's wavelength-resolved
Rayleigh/Mie inputs. Krisciunas & Schaefer remains the validated published
reference parameterization with fixed `k = 0.172 mag/airmass`; selecting a site
profile does not change its numerical result. The profile never selects or
rewrites the scientific `MoonlightModel`.

CLI location and scientific-profile selection are intentionally separate:

```bash
# CTAO-S geometry, generic Airglow maturity (default)
nsb point --site CTAO-S ...

# CTAO-S geometry with explicit CTAO-S planning assumptions
nsb point --site CTAO-S --site-profile cta-south ...
```

The second command selects a planning preset. Neither command claims a calibrated
CTAO-S Airglow model. In `nsb-cli`, CTAO presets are defined in
`crates/nsb-cli/src/site_profiles.rs`.

## Maturity invariants

The Airglow maturity selected by a site profile is not upgraded by
operational inputs. In particular, none of the following can promote a profile to
`Calibrated`:

- observer coordinates or observatory identity;
- Van Rhijn versus caller-provided vertical-emission geometry;
- automatic, dataset-backed, or explicit F10.7;
- atmospheric pressure/Rayleigh/Mie assumptions or extinction/scattering
  configuration; or
- caller-provided atmospheric planning assumptions.

Those values may change the numerical result or provenance fields. Calibration
maturity changes only through an explicit scientific profile/evidence path.
Typed public constructors create only `GenericFallback` or `PlanningPreset`
profiles; external `SiteProfileTag` implementations cannot construct
`Calibrated` through the public API. Airglow template identity and scaling are
internal because evaluation always uses the bundled continuum; profile `NAME`
metadata therefore cannot claim a custom or unevaluated Airglow asset.

## Validation contract

A profile may be promoted from `PlanningPreset` to `Calibrated` only after the
crate bundles or references reproducible site-specific validation inputs for:

1. surface pressure and altitude assumptions;
2. Rayleigh scale height;
3. aerosol/Mie optical-depth and phase-function parameters;
4. airglow continuum scale and temporal/seasonal corrections;
5. regression tests showing the calibrated profile changes moonlight and airglow
   predictions against documented reference data.

Until then, CTAO users should treat the application-layer CTA profiles as
explicit, inspectable planning defaults. Supplying a custom continuum or
operational scale alone is not an alternative calibration path.

## Versioned calibration asset

`SiteCalibrationAsset` schema v1 is the fail-closed evidence contract for future
calibrated site profiles (including CTAO-N and CTAO-S). A valid asset records:

- one stable calibration identifier and explicit site string;
- an inclusive date interval and wavelength domain;
- representative altitude, surface pressure, Rayleigh scale height, aerosol
  optical depth at 550 nm, and Angstrom exponent with one-sigma uncertainties;
- an airglow continuum scale and uncertainty, plus an explicit declaration of
  whether a temporal or seasonal correction is applied;
- one or more repository-relative immutable references with source, license and
  lowercase SHA-256;
- explicit scientific or operational limitations.

The parser rejects unknown fields, unsupported schemas, malformed identifiers,
invalid dates, non-finite or out-of-domain physical values, inconsistent airglow
correction metadata, duplicate references, unsafe paths, and missing provenance.
The schema rejects the reserved `generic-clear-sky` site id, so a generic
fallback cannot be mislabelled as a named-site calibration.

Example structure:

```toml
schema_version = 1
calibration_id = "ctao-south-reference-v1"
site = "ctao-south"
limitations = ["Valid only for the documented clear, moonless sample."]

[validity]
valid_from = "2025-01-01"
valid_through = "2025-12-31"
wavelength_nm = [300, 650]

[atmosphere]
representative_altitude_m = 2150.0
representative_altitude_uncertainty_m = 10.0
surface_pressure_hpa = 743.0
surface_pressure_uncertainty_hpa = 5.0
rayleigh_scale_height_km = 8.0
rayleigh_scale_height_uncertainty_km = 0.2
aerosol_optical_depth_550_nm = 0.03
aerosol_optical_depth_uncertainty_550_nm = 0.01
angstrom_exponent = 1.0
angstrom_exponent_uncertainty = 0.2

[airglow]
continuum_scale = 1.05
continuum_scale_uncertainty = 0.10
temporal_correction_applied = false

[[references]]
id = "ctao-south-atmosphere"
path = "site-calibration/ctao-south/atmosphere-v1.csv"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
source = "Documented CTAO-South atmospheric reference release"
license = "Redistribution terms recorded with the reference asset"
```

Parsing and validation are available through
`SiteCalibrationAsset::from_toml_str`. Passing this structural contract is
necessary but not sufficient for promotion. A later site-specific issue must
bundle the referenced bytes, define numerical validation tolerances, demonstrate
regressions against trusted observations, and explicitly connect the approved
asset to a new calibrated runtime profile. Existing CTAO North/South application
presets remain planning presets and issue #38 remains open.
