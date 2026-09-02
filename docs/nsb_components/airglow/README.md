# Airglow

Status: Current runtime-model guide.
Audience: Users and developers interpreting airglow outputs.
Scope: Empirical continuum, calculation inputs, geometry, calibration route, and limitations.

## What it is

Airglow is natural optical emission from Earth's upper atmosphere. Its intensity
varies with season, progression through the night, solar activity, viewing
geometry, wavelength, and local conditions. NSB uses a generic empirical
continuum baseline; it is not a line-by-line physical atmosphere simulation and
does not claim site calibration unless the caller supplies one.

## Evaluation stack

```text
continuum baseline
  x seasonal/time-of-night correction
  x F10.7 solar-activity correction
  x selected emitting-volume line-of-sight geometry
  x user/site scale
  -> independent Noll-2012 Rayleigh/Mie effective attenuation
  -> spectral and 300-650 nm photon-radiance outputs
```

Geometry and atmospheric attenuation are intentionally separate stages. A longer
path through an emitting layer is not the same physical effect as transmission
through the lower atmosphere. Uncertainty propagation uses the same selected
geometry multiplier as the nominal continuum.

The Noll effective extinction factors were fitted primarily for zenith distances
`z <= 60 deg`. NSB evaluates the same parametric form at larger angles but marks
that use as extrapolation with weaker upstream validation. Molecular absorption
from the full Cerro Paranal ASM/SkyCalc pipeline is not reproduced.

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
global production profile for all optical emission from 300 to 650 nm. See the
[optical vertical-profile audit](110-optical-vertical-profile-audit.md).

```rust
use nsb::{Airglow, AirglowGeometryModel, VanRhijnConfig};

let airglow = Airglow::standard_clear_sky(location)?
    .with_geometry(AirglowGeometryModel::VanRhijn(VanRhijnConfig::default()));
```

A persisted profile can be selected in the CLI with
`--airglow-vertical-profile profile.toml`. No network access is used to resolve
or evaluate it.

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

The direct/reference algorithm is retained as the runtime path. It has an
explicit, configurable even subdivision count for convergence testing and no
cache or interpolation layer.

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
with explicit uncalibrated provenance. Choosing an F10.7 source, extinction
model, or geometry model does not change that maturity to `Calibrated`.
Dedicated CTAO site calibration remains issue #38 and is deliberately separate.

## Issue relationship and scientific boundary

- #108 established the generic/planning baseline and audited site assumptions.
- #109 made F10.7 value and provenance deterministic and offline.
- #114 added the independent Noll Rayleigh/Mie effective attenuation stage.
- #110 adds explicit Van Rhijn and vertical-profile emitting-volume geometry.
- #112 is the parent completion audit for the machine-actionable component work.
- #38 remains the separate measurement-led CTAO site-calibration track.

Airglow has substantial natural variability. A different geometry model is not
by itself a more accurate prediction. Site/science use requiring calibrated
precision must supply documented measurements and validate the full model under
the intended conditions.

## Related documentation

- [Optical vertical-profile audit and numerical comparison](110-optical-vertical-profile-audit.md)
- [Airglow completion audit (#112)](112-completion-audit.md)
- [F10.7 solar-activity resolver](f107-resolver.md)
- [Generic baseline vs site calibration audit (#108)](108-audit-generic-baseline-vs-site-calibration.md)
- [Scientific metadata](../../specifications/scientific-metadata.md)
- [Model maturity](../../specifications/model-maturity.md)
