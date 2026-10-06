# Airglow

Status: Current runtime-model guide.
Audience: Users and developers interpreting airglow outputs.
Scope: Empirical continuum, calculation inputs, geometry, calibration route, and limitations.

## What it is

Airglow is natural optical emission from Earth's upper atmosphere. Its intensity
varies with season, progression through the night, solar activity, viewing
geometry, wavelength, and local conditions. NSB uses the PALACE v1.0 Paranal
unresolved-continuum model; it is not PALACE's line-by-line emission model and
the current runtime does not contain a validated dedicated Airglow site
calibration.

Airglow must not be forced through the celestial top-of-atmosphere direct path
in `nsb::transport`: emission originates inside the atmosphere, and the
component retains its emitting-volume geometry plus Noll effective
Rayleigh/Mie scattering. See
[Atmospheric transport](../../specifications/atmospheric-transport.md).

**Current policy:** NSB supports arbitrary-location Airglow evaluation through
`NsbEvaluator`, but PALACE is **Paranal-derived / Paranal-trained**. Without
explicit admitted site-calibration evidence it is a **generic/planning proxy**,
including when the observer is physically at Paranal.
A geographically generic API is not a globally calibrated dataset, and source
provenance is not calibration evidence for the source location. Geometry,
F10.7, atmosphere, extinction, or an explicit scale cannot upgrade maturity to
`Calibrated`.

Normal applications configure Airglow through `NsbModelConfig` and evaluate it
through `NsbEvaluator`. Direct construction of the internal Airglow component or
its continuum calibration is not part of the supported public API. The public
`components::airglow` route contains the scientific `AirglowModel` /
`AirglowSelection` contract plus advanced geometry types. The root API
re-exports `AirglowModel` and `AirglowSelection` for normal configuration.

## Automatic versus explicit selection

`NsbModelConfig` distinguishes **selection policy** from **model identity**:

| Concept | Type / API | Meaning |
| --- | --- | --- |
| Selection policy | `AirglowSelection::{Automatic, Explicit(model)}` | How the model was chosen |
| Explicit convenience | `with_airglow_model(model)` | Sets `Explicit(model)` |
| Inspection | `airglow_selection()`, `airglow_model()` | Policy and explicit request (if any) |
| Selection metadata | `NsbComponentMetadata::airglow_selection` | Kind, requested/resolved model, typed fallback |
| Evaluation outcome | `NsbComponentMetadata::airglow_evaluation` | Physical outcome after a query (absent from descriptors) |

Required behavior:

- **Explicit wins.** An explicit selection never silently switches to another
  model. The first-release `AirglowModel` enum contains only admitted models
  (`ParanalPalaceV1`). Future climatology (#157) adds a new
  `#[non_exhaustive]` variant when scientifically ready — no speculative public
  placeholder is frozen.
- **Automatic is deterministic.** `generic_clear_sky()`, `Default`, and
  `NsbEvaluator::new` use `AirglowSelection::Automatic`.
- **Automatic is not “Paranal is the global scientific default.”** Until a
  global climatological planning model is admitted (#157 deferred), automatic
  policy resolves to a **temporary Paranal-derived planning fallback**. That
  fallback is machine-visible via `used_automatic_fallback` and typed
  `AirglowFallbackReason::GlobalPlanningModelUnavailable`
  (`as_str()` → `global-planning-model-unavailable`).
- **Invalid inputs never become physical zero.** Altitude / F10.7 / scale are
  validated **before** astronomical-night gating. Outside night with valid
  inputs yields `AirglowPhysicalOutcome::PhysicalZero` with
  `AirglowPhysicalZeroReason::OutsideAstronomicalNight`.
- **Descriptors do not invent outcomes.** `describe_components()` populates
  selection metadata only; `airglow_evaluation` remains `None`.

`AirglowModel` remains the durable scientific identity enum
(`#[non_exhaustive]`). Future #157 climatology can extend the enum and refine
automatic resolution without replacing the `AirglowSelection` configuration
shape. The concrete continuum/evaluator remains internal.

## Geographic support versus scientific calibration

```text
Observatory / coordinates
        =
physical observer location

AirglowModel
        =
scientific Airglow model / parameterization

AirglowGeometryModel
        =
emitting-volume line-of-sight geometry

SiteProfile<P> / SiteProfileTag
        =
site assumptions and evidence-backed scientific maturity
```

These concerns are independent. Arbitrary valid Earth coordinates, named
observatories, and user-provided Siderust observatory catalogs are supported
geometrically. They default to `SiteProfile::<GenericClearSky>::generic_clear_sky()` unless
another site profile is selected explicitly. In particular:

- `--site PARANAL` does not create a calibrated Paranal Airglow result;
- `--site CTAO-N` does not select CTAO-North planning assumptions;
- `--site CTAO-S` does not select CTAO-South planning assumptions;
- `--site-profile cta-north` and `--site-profile cta-south` deliberately select
  application-layer planning assumptions, not calibrated products; and
- selecting a custom vertical-emission profile changes Airglow geometry only; it
  is not calibration evidence and does not upgrade scientific maturity.

Library users inspect the selected scientific maturity through
`NsbModelConfig` and result metadata:

```rust
use nsb::{AirglowSelection, CalibrationStatus, NsbModelConfig};

let config = NsbModelConfig::generic_clear_sky();
assert_eq!(config.airglow_selection(), AirglowSelection::Automatic);
assert_eq!(config.airglow_model(), None); // no explicit request
assert_eq!(config.site_profile_name(), "generic-clear-sky");
assert_eq!(
    config.airglow_calibration_status(),
    CalibrationStatus::GenericFallback,
);
assert!(!config.is_airglow_site_calibrated());
```

`airglow_selection()` reports the configuration policy before evaluation.
`airglow_model()` returns `Some` only for explicit selections. After evaluation,
`airglow_selection` reports resolved model / typed fallback and
`airglow_evaluation` reports the physical outcome. Site maturity
(`airglow_calibration_status()` / `is_airglow_site_calibrated()`) remains
independent of selection policy. Changing observer coordinates, F10.7,
geometry, or site maturity does not silently change the declared scientific
model identity.

## Evaluation stack

```text
selected AirglowModel
  -> three PALACE continuum templates (HO2, FeO-like, unresolved O2)
  x component/month/local-time/F10.7 climatology
  x selected emitting-volume line-of-sight geometry
  x user/site scale
  -> Noll-2012 Rayleigh/Mie effective transmission (once, spectrally)
  -> spectral and 300-650 nm photon-radiance outputs
```

The complete wavelength-dependent continuum expression before spectral
integration is

```text
Σ [template_c(λ) × PALACE_scale_c(month, local_time, F10.7)]
  × G(z) × Noll_scatter(λ) × user_scale
```

In code this is split so Noll scattering is applied exactly once:

- the PALACE stage preserves separate month/time/solar laws for all three
  continuum components and sums their physical R/nm spectra;
- `integrate_attenuated_continuum` applies geometry, user scale, and
  wavelength-dependent `Noll_scatter(λ)`, converts Rayleighs to photon
  radiance, and integrates over 300–650 nm.

There is not a second atmospheric-scattering multiplication after
`Noll_scatter(λ)`. Emitting-volume geometry (`G(z)` / Van Rhijn / vertical
profile) and atmospheric scattering/transmission (Noll Rayleigh/Mie) remain
conceptually distinct stages. Uncertainty propagation uses the same selected
geometry multiplier as the nominal continuum.

The Noll effective extinction factors were fitted primarily for zenith distances
`z <= 60 deg`. NSB evaluates the same parametric form at larger angles but marks
that use as extrapolation with weaker upstream validation. Molecular absorption
from the full Cerro Paranal ASM/SkyCalc pipeline is not reproduced.

## Continuum provenance and scope

The bundled `crates/nsb/data/airglow_palace_v1.dat` is deterministically derived
from PALACE v1.0 `palace_cont.fits` and `palace_var.fits`, released as model data
under CC BY 4.0. It covers the full NSB 300–650 nm band and retains all 12 months,
12 one-hour local-mean-solar-time bins, component-specific F10.7 slopes, residual
variability, and source layer heights. The GPL PALACE program and PALACE line
list are neither inputs to the derived bytes nor redistributed.

PALACE is based mainly on ten years of X-shooter observations at Cerro Paranal.
Its provenance is resolved, but its geographic applicability remains Paranal;
licensing clarity is not global calibration. See the
[generation and validation report](validation/palace-v1-runtime-product.md).

## Geometry models

`AirglowGeometryModel::VanRhijn(VanRhijnConfig)` is the default. It preserves the
fast, geometrically thin spherical-shell calculation at a representative 88 km
height for the PALACE continuum. PALACE's component heights (81/88/94 km) remain
recorded in the asset; the scalar geometry approximation is explicit in metadata.
The height is explicit
in advanced configuration and scientific metadata. The approximation does not
represent a layer's finite thickness, multiple emitting layers, or wavelength-
dependent emission altitude.

`AirglowGeometryModel::VerticalProfile(VerticalEmissionProfile)` integrates a
caller-provided relative volume-emission-rate profile through spherical Earth
geometry. It is opt-in because the available evidence does not justify one
global production profile for all optical emission from 300 to 650 nm.

Advanced library configuration uses the public geometry types under
`components::airglow` and applies them through `NsbModelConfig`:

```rust
use nsb::components::airglow::{AirglowGeometryModel, VanRhijnConfig};
use nsb::NsbModelConfig;

let config = NsbModelConfig::generic_clear_sky().with_airglow_geometry(
    AirglowGeometryModel::VanRhijn(VanRhijnConfig::default()),
);
```

Evaluation still goes through `NsbEvaluator`; the geometry types are not a
second direct component-evaluation API. A persisted profile can be selected in
the CLI with `--airglow-vertical-profile profile.toml`. No network access is
used to resolve or evaluate it. Selecting either geometry leaves the selected
site profile and its calibration status unchanged.

### Why no bundled broadband VER profile

Optical 300–650 nm airglow mixes physically different sources (for example
OI 557.7 nm near ~90–100 km, OI 630.0/636.4 nm near ~200–400 km, Na D near
~90 km, O₂ bands near ~91–95 km, FeO-like continuum near ~85–89 km, plus other
continua). Species, latitude, season, local time, and solar activity do not
necessarily vary together. Line-specific public products (ICON/MIGHTI, WINDII)
and Paranal X-shooter continuum climatology inform this limitation; infrared
limb products such as SABER are not optical ground truth for this band.

NSB therefore uses PALACE's middle continuum-layer height of 88 km as the
scalar Van Rhijn default, accepts validated
checksum-pinned caller profiles with provenance/licence/applicability, and makes
no claim that selecting advanced geometry improves accuracy by itself.
Measurement-led CTAO profiles belong to issue #38. Durable source evidence,
candidate-data assessment, licence notes, and cross-model validation numbers are
retained in the
[optical vertical-profile decision record](validation/optical-vertical-profile-decision-v1.md).

## Spherical vertical-profile formulation

For observer radius `r0 = R_E + h_obs`, zenith angle `z`, and distance `s` along
the ray, the altitude sampled by the integrator is

```text
h(s) = sqrt(r0^2 + s^2 + 2 r0 s cos(z)) - R_E.
```

NSB integrates the piecewise-linear emissivity `j(h(s))` over the exact ray
segments intersecting the profile altitude bounds, using composite Simpson
quadrature, and reports

```text
G(z) = integral_LOS j(h(s)) ds / integral_zenith j(h(s)) ds.
```

The zenith result is exactly normalized to one. The observer altitude comes from
the supplied `Geodetic<ECEF>` location; no observatory altitude is hidden in the
model. The supported domain is above the geometric horizon (`0 <= z <= 90 deg`)
and may be narrowed by profile metadata. Consistent with the existing Airglow
component contract, valid targets below the apparent horizon but above altitude
`-90 deg` use the horizon geometry; the nadir endpoint and invalid coordinates
produce zero component output.

At the geometric horizon a thin shell produces altitude-dependent factors
(approximately 6.012, 6.097, and 6.185 at observer altitudes 0, 2.5, and 5 km).
That dependence is expected from spherical ray geometry and differs from the
observer-altitude-independent historical Van Rhijn formula. Cross-model and
resolution-convergence checks remain internal validation tests so the numerical
integrator is not part of the public API contract.

The direct/reference algorithm is retained as the runtime path. Its subdivision
count is an internal convergence/performance choice rather than caller
configuration. Benchmark numbers live in the
[performance contract](../../specifications/performance.md).

## Vertical profile contract

`VerticalEmissionProfile` validates before it can be evaluated. A persisted
scientific profile must provide:

- schema version and profile identifier;
- a strictly increasing altitude grid in kilometres with at least three points;
- finite, non-negative relative emissivities with positive total emission;
- the `unit-vertical-integral` normalization convention;
- wavelength/band applicability that includes the NSB 300-650 nm output band;
- assumptions/reference state, provenance/reference, and licence information;
- a validated zenith-angle domain; and
- a matching deterministic `sha256:` identity over canonical normalized data
  and metadata.

Unsupported versions or applicability, missing persisted provenance/checksum,
duplicate or unsorted bins, non-finite values, and invalid normalization fail
closed. Programmatically constructed profiles receive a deterministic checksum;
persisted profiles must pin and reproduce it.

## F10.7 and calibration

PALACE was fitted using centred 27-day F10.7 averages and its training data span
67–166 sfu. The current automatic path resolves NSB's documented monthly
planning F10.7 quantity from the bundled offline store for the evaluation UTC
date. This cadence mismatch is a stated approximation. Callers can set a value with
`NsbModelConfig::with_solar_radio_flux` or `--solar-radio-flux-sfu`. See the
[F10.7 resolver](f107-resolver.md).

The generic and CTAO planning profiles use the PALACE-derived continuum baseline
with explicit uncalibrated provenance. Automatic F10.7, an explicit value, or a
pinned dataset changes solar-activity provenance only. Likewise, selecting an
atmosphere/extinction model, Airglow geometry, observer coordinates, or user
scale does not change maturity to `Calibrated`. Dedicated CTAO site calibration
remains issue #38 and is deliberately separate.

## Scientific boundary

Airglow has substantial natural variability. A different geometry model is not
by itself a more accurate prediction. Site/science use requiring calibrated
precision requires documented measurements, admitted calibration evidence, and
validation of the full model under the intended conditions. The current runtime
has no path that promotes generic/CTAO Airglow to `Calibrated`; CTAO promotion
remains blocked by issue #38. Machine-actionable Airglow geometry, F10.7
resolution, and attenuation stages are complete, while scientific
representativeness remains explicitly limited.

## Related documentation

- [Scientific evidence and decision records](validation/README.md)
- [F10.7 solar-activity resolver](f107-resolver.md)
- [Scientific metadata](../../specifications/scientific-metadata.md)
- [Model maturity](../../specifications/model-maturity.md)
- [Validation matrix](../../specifications/validation.md)
- [Performance contract](../../specifications/performance.md)
- [CTAO site profiles](../../specifications/ctao-site-profiles.md)
