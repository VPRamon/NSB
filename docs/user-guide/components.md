# Runtime components

Status: Current runtime component overview.
Audience: Users selecting model components and developers interpreting results.
Scope: Scientific role, runtime dependencies, model choices, and maturity boundaries.

For a component-by-component explanation of the physical source, calculation,
offline generation where applicable, and scientific boundaries, see the
[NSB component guides](../nsb_components/README.md).

## Composition model

NSB evaluates the total night-sky background as the sum of independently
reported contributors:

```text
total NSB = zodiacal light + integrated starlight + airglow + scattered moonlight
```

Each component has different dependencies on direction, time, atmosphere, and
scientific assets. NSB therefore reports both the total and the individual
contributions with their own maturity, provenance, validated domain, and
uncertainty metadata.

## Component summary

| Component | Physical origin | Main dependencies | Runtime implementation | Default behaviour |
| --- | --- | --- | --- | --- |
| Zodiacal light | Sunlight scattered by interplanetary dust | Ecliptic geometry, solar spectrum, line-of-sight extinction | Leinert brightness grid with solar spectrum and optional Noll-style atmospheric extinction | Included in `all` |
| Integrated starlight | Unresolved stellar flux represented by a Galactic HEALPix product | Galactic direction and the selected map/manifest | Bundled validated production map, validated external map, or explicit experimental map/seed | Included in `all` only when a production asset is embedded |
| Airglow | Emission from the upper atmosphere | Season, time within the night, solar activity, zenith angle, site profile | Empirical continuum with temporal, solar, and Van Rhijn corrections | Included in `all` |
| Moonlight | Lunar light scattered by the atmosphere | Moon phase and geometry, target separation, atmosphere, wavelength | Jones et al. (2013) spectral model or KS91 analytic V-band reference | Included in `all` |

## Zodiacal light

The zodiacal component combines a directional brightness model with a reference
solar spectrum. Atmospheric extinction can be applied using the default
Noll-style approximation or disabled explicitly.

CLI selection:

```bash
--components zodiacal
--zodiacal-extinction noll2012
```

Use `--zodiacal-extinction none` only when the caller intentionally wants the
unattenuated model contribution. The default component metadata describes a
generic clear-sky scientific surface rather than an observatory-specific
calibration.

## Integrated starlight

Starlight is a directional data product rather than a catalogue query performed
at runtime. Runtime NSB never downloads Gaia or another stellar catalogue.

The supported selections are deliberately distinct:

- **bundled production starlight**: selected by `starlight` when a validated
  production map is embedded in the build;
- **validated external starlight**: selected with `starlight` plus both
  `--starlight-map` and `--starlight-manifest`;
- **bundled experimental seed**: selected only by
  `experimental-starlight`;
- **experimental library map**: supplied through
  `StarlightModel::with_experimental_map`.

There is no fallback from production starlight to the experimental seed. Missing
or invalid production evidence is an error.

See the [external manifest contract](../nsb_components/starlight/external-manifest.md) and the
[starlight data-product pipeline](../nsb_components/starlight/map-generation.md).

## Airglow

The airglow model uses a bundled continuum template and applies empirical
corrections for observing geometry and temporal conditions. Its solar-activity
input is F10.7 radio flux in solar flux units.

CLI selection and override:

```bash
--components airglow
--solar-radio-flux-sfu 130
```

When no value is supplied, NSB uses the documented default. Built-in site
profiles may select explicit planning assumptions, but the current CTAO profiles
do not include dedicated site-calibrated airglow continua.

## Scattered moonlight

Two model choices are available:

| CLI value | Library model | Intended use |
| --- | --- | --- |
| `jones2013` | `MoonlightModel::Jones2013Spectral` | Default wavelength-resolved model used for integrated NSB evaluation |
| `ks1991` | `MoonlightModel::KrisciunasSchaefer1991` | Published analytic V-band reference and comparison model |

```bash
--components moon
--moonlight-model jones2013
```

Atmospheric assumptions are selected through the model site profile. The Jones
implementation and its validation limits are documented in
[Jones 2013 moonlight](../nsb_components/moonlight/jones2013-validation.md).

## Meaning of `all`

`ComponentMask::ALL`, `ComponentMask::DEFAULT`, and CLI `--components all` are
the same production-safe composition.

- Without an embedded validated production starlight asset, `all` contains
  zodiacal light, airglow, and moonlight.
- With an embedded validated production starlight asset, `all` also contains
  starlight.
- The experimental seed is never part of `all`.

Downstream systems should record the returned model version and component list
instead of assuming that `all` always expands to a fixed number of components.

## Scientific maturity

Software readiness and scientific calibration are separate. A component may be
reliable as software while still being a generic model, planning preset,
published reference, proxy, or experimental product. Use
[Model maturity](../specifications/model-maturity.md), [Scientific metadata](../specifications/scientific-metadata.md),
and the [Validation matrix](../specifications/validation.md) when deciding whether a result is
appropriate for scientific or operational use.
