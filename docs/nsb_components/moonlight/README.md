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

`Jones2013Spectral` is the default wavelength-resolved model used by integrated
NSB evaluation. It combines the Jones et al. (2013) lunar formulation supplied
by Siderust with NSB's solar spectrum, Mie phase grid, and
multiple-scattering correction grid. `KrisciunasSchaefer1991` is a published
analytic V-band reference model, intended for comparison and V-band use.

For non-observable geometries, including a Moon or target below the horizon,
the component returns zero.

## Atmospheric inputs and site profiles

The Jones model takes surface pressure, Rayleigh scale height, and Mie/aerosol
parameters. Observer altitude always comes from the supplied location, avoiding
an inconsistent combination of an atmosphere profile from one site with another
site's altitude. `standard_clear_sky` is an altitude-derived generic fallback;
named CTAO profiles are explicit planning presets. Neither substitutes for a
site-calibrated aerosol model.

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
