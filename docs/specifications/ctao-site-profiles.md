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
