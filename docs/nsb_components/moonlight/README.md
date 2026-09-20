# Scattered moonlight

Status: Current runtime-model guide.
Audience: Users and developers selecting or interpreting moonlight models.
Scope: Model choices, calculation geometry, atmospheric inputs, and validation limits.

## What it is

Moonlight is solar light reflected by the Moon and then scattered through
Earth's atmosphere into the target line of sight. It depends strongly on lunar
phase, Moon-target separation, the Moon and target zenith distances, lunar
distance, wavelength, and atmospheric aerosol properties.

## How NSB calculates it

For both available models, NSB derives the observing geometry internally from
the UTC time, observer location, and ICRS/J2000 target:

```text
time + site + target
  -> lunar phase, topocentric distance, Moon/target zenith distances, separation
  -> reflected lunar source radiance
  -> Rayleigh and aerosol (Mie) atmospheric scattering
  -> 300–650 nm photon radiance and B/V diagnostics
```

`MoonlightModel` is the stable scientific selection contract. Its
`Jones2013Spectral` variant is the deterministic default wavelength-resolved
model used by integrated NSB evaluation. It combines the Jones et al. (2013)
lunar formulation supplied by Siderust with NSB's solar spectrum, Mie phase
grid, and multiple-scattering correction grid. The
`KrisciunasSchaefer1991` variant remains a deliberately supported published
analytic V-band reference and comparison model.

Concrete evaluator implementation types are internal. Library callers select a
model through `NsbModelConfig::with_moonlight_model` and evaluate Moonlight
through `NsbEvaluator`; component-specific `compute` and range-search APIs are
not part of the supported contract.

For non-observable geometries, including a Moon or target below the horizon,
the component returns zero.

## Atmospheric inputs and site profiles

The two scientific models deliberately treat atmospheric configuration
differently.

Jones uses the selected `SiteProfileId` profile's surface pressure, Rayleigh
scale height, and Mie/aerosol parameters wavelength by wavelength. Its geometric
altitude remains the actual query observer altitude.

Krisciunas & Schaefer remains the published analytic V-band reference
parameterization validated by the repository regression fixture. It uses the
fixed `k = 0.172 mag/airmass` reference extinction used by that validation;
selecting a different `SiteProfileId` does not rewrite this parameter or change
the K&S numerical result. The selected profile may still be reported alongside
the model in provenance so callers can audit the complete evaluator
configuration without implying that the profile calibrates the K&S reference.

Named CTAO profiles remain planning assumptions rather than site-calibrated
aerosol models. Site-profile selection and `MoonlightModel` scientific identity
are independent configuration concepts.

The implementation includes `JONES_MIE_WEIGHT = 0.05`, an empirical correction
for its simplified scattering path and bundled phase grid. It is not a physical
constant and must be revalidated if changed.

## Scientific boundaries

The documented validation domain is 300–650 nm under clear-sky conditions with
the Moon and target above the horizon and positive separation. Existing
regression-fixture tolerances are capped at 20%; they do not demonstrate
independent SkyCalc agreement or dedicated CTAO aerosol calibration.

## Related documentation

- [Jones 2013 spectral moonlight validation](jones2013-validation.md)
- [CTAO site profiles](../../specifications/ctao-site-profiles.md)
- [Validation matrix](../../specifications/validation.md)
