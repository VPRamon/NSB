# Zodiacal light

Status: Current runtime-model guide.
Audience: Users and developers interpreting zodiacal-light outputs.
Scope: Physical origin, calculation path, atmospheric treatment, and model limits.

## What it is

Zodiacal light is sunlight scattered by micron-scale dust in the inner Solar
System. It is directional: it is generally brightest near the ecliptic plane
and at small angular distance from the Sun. NSB models the celestial source and
the optional propagation through the observer's atmosphere separately.

## How NSB calculates it

The default `ZodiacalLight::leinert1998()` path is:

```text
UTC time + target direction
  -> target ecliptic latitude and longitude offset from the Sun
  -> Leinert et al. (1998) S10 brightness lookup
  -> scale bundled solar spectrum at 500 nm
  -> apply Leinert wavelength reddening
  -> optionally apply Noll et al. (2012) atmospheric extinction
  -> convert energy radiance to photon radiance
  -> integrate 300–650 nm
```

The Leinert lookup interpolates the brightness table in absolute ecliptic
latitude and absolute Sun-relative ecliptic longitude. The source brightness is
expressed in S10 units before it is converted to a spectrum. For an observed
ground-based value, NSB derives the target altitude and returns zero below the
horizon.

`compute_exoatmospheric` evaluates only the celestial source. `compute_observed`
and `compute` include the selected extinction treatment. The default is the
Noll-2012 approximation; `ZodiacalExtinction::None` is available when the
unattenuated source contribution is explicitly required.

## Inputs and generated data

This component does not use an offline catalogue-generation pipeline. Its
runtime inputs are the built-in Leinert brightness table and the bundled solar
reference spectrum. A caller may supply a validated rectangular
`ZodiacalBrightnessGrid` or replacement solar spectrum; custom inputs must
carry their own scientific provenance and validation.

## Scientific boundaries

The default model is an empirical directional brightness model, not a
site-calibrated all-sky measurement. Its atmospheric correction is an explicit
approximation and should not be read as a complete local aerosol model. Results
should be interpreted together with their returned maturity and provenance
metadata.

## References and related documentation

- Leinert et al. (1998), *A&AS* 127, 1–99: empirical zodiacal-light table.
- Noll et al. (2012), *A&A* 543, A92: atmospheric-extinction approximation.
- [Runtime component overview](../../user-guide/components.md)
- [Scientific metadata](../../specifications/scientific-metadata.md)
