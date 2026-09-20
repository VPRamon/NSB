# Zodiacal light

Status: Current runtime-model guide.
Audience: Users and developers interpreting zodiacal-light outputs.
Scope: Scientific source model, atmospheric propagation, provenance, and model limits.

## Public configuration

Normal applications configure Zodiacal light through `NsbModelConfig` and
evaluate it through `NsbEvaluator`. The first-release scientific selector is:

```rust
use nsb::{NsbModelConfig, ZodiacalExtinction, ZodiacalModel};

let config = NsbModelConfig::default()
    .with_zodiacal_model(ZodiacalModel::Leinert1998)
    .with_zodiacal_extinction(ZodiacalExtinction::Noll2012Approx);

assert_eq!(config.zodiacal_model().as_str(), "leinert-1998");
assert_eq!(
    config.zodiacal_extinction().as_str(),
    "noll-2012-approximation"
);
```

`ZodiacalModel` identifies the celestial scientific source model.
`ZodiacalExtinction` is an independent atmospheric-propagation choice. Changing
propagation does not change the selected source model.

## Default scientific model

`ZodiacalModel::Leinert1998` is the deterministic default. Its calculation path
is:

```text
UTC time + target direction
  -> target ecliptic latitude and longitude offset from the Sun
  -> Leinert et al. (1998) S10 brightness lookup
  -> scale the bundled solar reference spectrum at 500 nm
  -> apply Leinert wavelength reddening
  -> apply the selected atmospheric propagation
  -> convert energy radiance to photon radiance
  -> integrate 300–650 nm
```

The Leinert lookup interpolates the brightness table in absolute ecliptic
latitude and absolute Sun-relative ecliptic longitude. B/V diagnostics are
central-wavelength S10 proxies at 445 nm and 551 nm. Ground-observer evaluation
returns zero for targets below the horizon.

## Atmospheric propagation

The default is `ZodiacalExtinction::Noll2012Approx`, the repository's existing
Noll et al. (2012)-style Rayleigh/Mie attenuation approximation.
`ZodiacalExtinction::None` applies no atmospheric attenuation. The latter is
useful when a caller intentionally wants the unattenuated contribution or
handles propagation outside NSB. It remains a ground-observer evaluation:
horizon visibility is still enforced, so `None` is not an exoatmospheric mode.

The Noll approximation is generic rather than site-calibrated. Selecting it
must not be interpreted as evidence for a local aerosol profile. Runtime
metadata records whether Noll attenuation or no attenuation was actually used.

## Inputs and provenance

The first-release `Leinert1998` model owns its bundled Leinert brightness
table and bundled solar reference spectrum as implementation/provenance inputs.
They are not independently replaceable through the stable application API.

Earlier pre-release code exposed caller-defined `ZodiacalBrightnessGrid`,
`ZodiacalBrightnessModel`, `ZodiacalLight`, and solar-spectrum replacement.
Those paths did not provide a scientific admission contract, did not participate
correctly in evaluator configuration/provenance, and created a second evaluation
API. They are therefore not part of the first stable surface.

A future custom grid, custom solar spectrum, alternative scientific model, or
site-calibrated extinction path should first define validation, admission, and
provenance semantics rather than re-exposing raw implementation dependencies.

## Scientific boundaries

The default model is an empirical directional brightness model, not a
site-calibrated all-sky measurement. Its atmospheric correction is an explicit
approximation. Results should be interpreted together with their returned
maturity and provenance metadata.

## References and related documentation

- Leinert et al. (1998), *A&AS* 127, 1–99: empirical zodiacal-light table.
- Noll et al. (2012), *A&A* 543, A92: atmospheric-extinction approximation.
- [Runtime component overview](../../user-guide/components.md)
- [Scientific metadata](../../specifications/scientific-metadata.md)
