# Airglow

Status: Current runtime-model guide.
Audience: Users and developers interpreting airglow outputs.
Scope: Empirical continuum, calculation inputs, geometry, calibration route, and limitations.

## What it is

Airglow is natural optical emission from Earth's upper atmosphere. Its intensity
varies with season, progression through the night, solar activity, viewing
geometry, wavelength, and local conditions. NSB uses a generic empirical
continuum baseline; it is not a line-by-line physical atmosphere simulation and
the current runtime does not contain a validated dedicated Airglow site
calibration.

**Option D (current policy):** NSB exposes an arbitrary-location Airglow
evaluator, but the empirical continuum is **Paranal-derived / Paranal-trained**
(Noll/SkyCalc lineage, including FORS1 residual continuum heritage). Without an
explicit validated scientific profile it is an **explicit generic/planning
proxy**, including when the observer is physically at Paranal. A geographically
generic API is not a globally calibrated dataset, and source provenance is not
calibration evidence for the source location. Geometry, F10.7, atmosphere,
extinction, or an explicit scale cannot upgrade maturity to `Calibrated`.

## Geographic support versus scientific calibration

```text
Observatory / coordinates
        =
physical observer location and geometry

SiteProfileId / AirglowScientificProfile
        =
NSB assumptions and evidence-backed scientific maturity
```

These concerns are independent. Arbitrary valid Earth coordinates, named
observatories, and user-provided Siderust observatory catalogs are supported
geometrically. They default to `SiteProfileId::GenericClearSky` unless the
scientific profile is selected explicitly. In particular:

- `--site PARANAL` does not create a calibrated Paranal Airglow result;
- `--site CTAO-N` does not select `SiteProfileId::CtaNorth`;
- `--site CTAO-S` does not select `SiteProfileId::CtaSouth`;
- `--site-profile cta-north` and `--site-profile cta-south` deliberately select
  planning assumptions, not calibrated products; and
- supplying a custom `AirglowContinuum` is classified as an unvalidated custom
  continuum, not as calibration evidence.

Direct component users can inspect the scientific state programmatically:

```rust
use nsb::{
    Airglow, AirglowScientificProfile, CalibrationStatus, SiteProfileId,
};

let airglow = Airglow::standard_clear_sky(location)?;
assert_eq!(
    airglow.scientific_profile(),
    AirglowScientificProfile::BuiltIn(SiteProfileId::GenericClearSky),
);
assert_eq!(airglow.calibration_status(), CalibrationStatus::GenericFallback);
assert!(!airglow.is_site_calibrated());
```

Evaluator users can inspect the same scientific choice through
`NsbModelConfig::airglow_scientific_profile()`,
`airglow_calibration_status()`, and `is_airglow_site_calibrated()`. Component
result metadata derives its structured calibration status from the selected
site profile's `CalibrationStatus`; geometry and solar-activity provenance are
reported separately.

## Evaluation stack

```text
continuum baseline
  x seasonal/time-of-night correction
  x F10.7 solar-activity correction
  x selected emitting-volume line-of-sight geometry
  x user/site scale
  -> Noll-2012 Rayleigh/Mie effective transmission (once, spectrally)
  -> spectral and 300-650 nm photon-radiance outputs
```

The complete wavelength-dependent continuum expression before spectral
integration is

```text
global_scale × solar_corr × seasonal_corr × G(z) × Noll_scatter(λ) × user_scale
```

In code this is split so Noll scattering is applied exactly once:

- `scalar_scale` holds `global_scale × solar_corr × seasonal_corr × G(z) × user_scale`
  (scalar corrections and emitting-volume LOS geometry);
- `integrate_attenuated_continuum` applies `Noll_scatter(λ)` as wavelength-
  dependent Rayleigh/Mie transmission, then integrates over 300–650 nm.

There is not a second atmospheric-scattering multiplication after
`Noll_scatter(λ)`. Emitting-volume geometry (`G(z)` / Van Rhijn / vertical
profile) and atmospheric scattering/transmission (Noll Rayleigh/Mie) remain
conceptually distinct stages. Uncertainty propagation uses the same selected
geometry multiplier as the nominal continuum.

The Noll effective extinction factors were fitted primarily for zenith distances
`z <= 60 deg`. NSB evaluates the same parametric form at larger angles but marks
that use as extrapolation with weaker upstream validation. Molecular absorption
from the full Cerro Paranal ASM/SkyCalc pipeline is not reproduced.

## Continuum provenance and UV-end limits

The bundled continuum (`crates/nsb/data/airglow_cont.dat`) inherits Paranal
training assumptions (seasonal and time-of-night matrices; solar slope/constant;
effective emitting-shell height 90 km). Upstream FORS1 windows are roughly
0.365–0.89 µm and are weaker below ~0.44 µm; asset headers note extra uncertainty
below ~0.4 µm and above ~0.9 µm. NSB's 300–650 nm band therefore inherits
elevated uncertainty toward ~300–365/400 nm. NSB does not invent an extra UV
envelope beyond the continuum's reported relative uncertainty.

Exact upstream import file/release and some licence details remain unresolved
where the asset registry records them; treat those as release limitations, not
as calibrated global science. There is no Paranal/CTAO location whitelist:
location is a caller input while the continuum remains Paranal-derived. The
Paranal lineage explains where the empirical baseline came from; it does not
constitute a dedicated, admitted Paranal site-calibration contract.

## Geometry models

`AirglowGeometryModel::VanRhijn(VanRhijnConfig)` is the default. It preserves the
previous NSB calculation exactly: a fast, geometrically thin spherical shell at
an effective height of 90 km for the standard continuum. The height is now
explicit in configuration and scientific metadata. The approximation does not
represent a layer's finite thickness, multiple emitting layers, or wavelength-
dependent emission altitude.

`AirglowGeometryModel::VerticalProfile(VerticalEmissionProfile)` integrates a
caller-provided relative volume-emission-rate profile through spherical Earth
geometry. It is opt-in because the available evidence does not justify one
global production profile for all optical emission from 300 to 650 nm.

```rust
use nsb::{Airglow, AirglowGeometryModel, VanRhijnConfig};

let airglow = Airglow::standard_clear_sky(location)?
    .with_geometry(AirglowGeometryModel::VanRhijn(VanRhijnConfig::default()));
```

A persisted profile can be selected in the CLI with
`--airglow-vertical-profile profile.toml`. No network access is used to resolve
or evaluate it. Selecting either geometry leaves `AirglowScientificProfile` and
its calibration status unchanged.

### Why no bundled broadband VER profile

Optical 300–650 nm airglow mixes physically different sources (for example
OI 557.7 nm near ~90–100 km, OI 630.0/636.4 nm near ~200–400 km, Na D near
~90 km, O₂ bands near ~91–95 km, FeO-like continuum near ~85–89 km, plus other
continua). Species, latitude, season, local time, and solar activity do not
necessarily vary together. Line-specific public products (ICON/MIGHTI, WINDII)
and Paranal X-shooter continuum climatology inform this limitation; infrared
limb products such as SABER are not optical ground truth for this band.

NSB therefore keeps the 90 km Van Rhijn default, accepts validated
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
observer-altitude-independent historical Van Rhijn formula. Cross-model
comparisons are available via
`cargo run -p nsb --example airglow_geometry_comparison`.

The direct/reference algorithm is retained as the runtime path. It has an
explicit, configurable even subdivision count for convergence testing and no
cache or interpolation layer. Benchmark numbers live in the
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

The automatic path resolves monthly-averaged F10.7 from the bundled offline
store for the evaluation UTC date. Callers can set an explicit value with
`with_solar_radio_flux` / `with_f10_7` or `--solar-radio-flux-sfu`. See the
[F10.7 resolver](f107-resolver.md).

The generic and CTAO planning profiles use a SkyCalc-derived continuum baseline
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
