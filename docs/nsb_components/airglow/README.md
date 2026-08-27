# Airglow

Status: Current runtime-model guide.
Audience: Users and developers interpreting airglow outputs.
Scope: Empirical continuum, calculation inputs, calibration route, and limitations.

## What it is

Airglow is natural optical emission from Earth's upper atmosphere. Its intensity
varies with season, progression through the night, solar activity, viewing
geometry, and local conditions. NSB represents it as a site-bound empirical
continuum rather than a line-by-line physical atmosphere simulation.

## How NSB calculates it

```text
site + UTC time + target direction + F10.7 solar radio flux
  -> target altitude
  -> seasonal and time-of-night continuum terms
  -> solar-activity correction
  -> Van Rhijn viewing-geometry correction
  -> configured site scale
  -> wavelength-resolved and 300–650 nm photon-radiance outputs
```

The default automatic path resolves F10.7 from the bundled offline store for the
evaluation UTC date (observations, forecasts, then climatology). Callers may set
an explicit value with `with_solar_radio_flux` / `with_f10_7` or the CLI
`--solar-radio-flux-sfu` option. See [F10.7 resolver](f107-resolver.md).

## Inputs and calibration

The standard model loads a bundled SkyCalc-derived empirical continuum template.
`Airglow::for_site_profile` records the selected site assumptions and applies
the profile scale. A custom, site-calibrated `AirglowContinuum` can be supplied
with `Airglow::with_continuum`.

The CTAO profiles currently use the bundled continuum with neutral scale and
explicit uncalibrated provenance. They are useful planning assumptions, not
dedicated site measurements.

## Scientific boundaries

Airglow has substantial natural variability that an empirical continuum cannot
fully predict for a particular night. The generic and CTAO planning paths must
not be interpreted as real-time monitoring or site-specific calibration. For
science use requiring that precision, provide a documented local continuum and
validate it against the intended conditions.

## Related documentation

- [F10.7 solar-activity resolver](f107-resolver.md)
- [Generic baseline vs site calibration audit (#108)](108-audit-generic-baseline-vs-site-calibration.md)
- [Runtime component overview](../../user-guide/components.md)
- [CTAO site profiles](../../specifications/ctao-site-profiles.md)
- [Model maturity](../../specifications/model-maturity.md)
- [Scientific metadata](../../specifications/scientific-metadata.md)
