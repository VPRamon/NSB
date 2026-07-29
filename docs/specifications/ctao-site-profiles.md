# CTAO site-profile assumptions

Status: Current planning-preset contract.
Audience: CTAO planning users, reviewers, and maintainers.
Scope: Built-in site profile assumptions, exposed metadata, and promotion
requirements.
Non-goals: This document does not claim site calibration for CTAO-N or CTAO-S.

NSB distinguishes generic clear-sky fallbacks from named site profiles through
`SiteProfileId`, `SiteProfile`, `CalibrationStatus`, and the
`NsbModelConfig::cta_n_planning()` / `NsbModelConfig::cta_s_planning()` evaluator
presets.

The current CTAO entries are **planning presets**, not fully site-calibrated
science products. They exist so CTAO call sites can select explicit assumptions
and inspect provenance instead of implicitly using `standard_clear_sky`.

## Built-in profiles

| Profile | Status | Atmosphere | Airglow |
| --- | --- | --- | --- |
| `SiteProfileId::GenericClearSky` | `GenericFallback` | Pressure derived from observer altitude; default Rayleigh scale height; bundled clear-sky Mie parameters. | Bundled `NSB/data/airglow_cont.dat` continuum with neutral scale. |
| `SiteProfileId::CtaNorth` | `PlanningPreset` | Representative La Palma/ORM altitude, fixed planning pressure of 770 hPa, default Rayleigh scale height, Paranal-like bundled Mie parameters. | Bundled `NSB/data/airglow_cont.dat` continuum with neutral scale; no CTA-N-specific continuum calibration is bundled yet. |
| `SiteProfileId::CtaSouth` | `PlanningPreset` | Paranal-like `AtmosphereProfile::EL_PARANAL` assumptions used explicitly for the Paranal/Atacama CTAO use case. | Bundled `NSB/data/airglow_cont.dat` continuum with neutral scale; no CTA-S-specific continuum calibration is bundled yet. |

## API usage

Evaluator-level CTAO planning preset:

```rust
use nsb::{NsbEvaluator, NsbModelConfig};

let evaluator = NsbEvaluator::with_config(NsbModelConfig::cta_s_planning())?;
```

Component-level explicit profile selection:

```rust
use nsb::{Airglow, Jones2013Spectral, SiteProfileId};

let moonlight = Jones2013Spectral::for_site_profile(observer, SiteProfileId::CtaSouth);
let airglow = Airglow::for_site_profile(observer, SiteProfileId::CtaSouth)?;
let profile = SiteProfileId::CtaSouth.profile(observer);

assert_eq!(profile.calibration_status, nsb::CalibrationStatus::PlanningPreset);
assert!(!profile.is_site_calibrated());
```

## Validation contract

A profile may be promoted from `PlanningPreset` to `Calibrated` only after the
crate bundles or references reproducible site-specific validation inputs for:

1. surface pressure and altitude assumptions;
2. Rayleigh scale height;
3. aerosol/Mie optical-depth and phase-function parameters;
4. airglow continuum scale and temporal/seasonal corrections;
5. regression tests showing the calibrated profile changes moonlight and airglow
   predictions against documented reference data.

Until then, CTAO users should treat the built-in CTA profiles as explicit,
inspectable planning defaults and pass custom component profiles when they need
validated operational thresholds.

## Versioned calibration asset

`SiteCalibrationAsset` schema v1 is the fail-closed evidence contract for future
CTAO-N and CTAO-S calibrated profiles. A valid asset records:

- one stable calibration identifier and explicit CTAO site;
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
The schema cannot represent `GenericClearSky`, so a generic fallback cannot be
mislabelled as a named-site calibration.

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
asset to a new calibrated runtime profile. Existing `CtaNorth` and `CtaSouth`
profiles remain planning presets.
