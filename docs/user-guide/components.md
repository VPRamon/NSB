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
| Airglow | Emission from the upper atmosphere | Season, time within the night, solar activity, zenith angle, observer altitude, geometry/profile, site profile | Empirical continuum with temporal and solar terms, selectable emitting-volume geometry, and independent Noll attenuation | Included in `all` |
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
- **experimental library map**: supplied through
  `StarlightModel::with_experimental_map`.

There is no bundled experimental seed. Missing or invalid production evidence is
an error.

See the [external manifest contract](../nsb_components/starlight/external-manifest.md) and the
[starlight data-product pipeline](../nsb_components/starlight/map-generation.md).

## Airglow

The airglow model uses a bundled continuum template and applies empirical
corrections for observing geometry and temporal conditions. Van Rhijn is the
unchanged default geometry: an explicit thin shell at 90 km. Callers can instead
provide a validated, checksum-pinned vertical emission profile; the spherical
line-of-sight integration uses the observer's real altitude. Geometry remains
separate from the wavelength-dependent Noll atmospheric attenuation stage.

Its solar-activity
input is F10.7 radio flux in solar flux units. By default NSB resolves F10.7
from the bundled offline store for the evaluation UTC date; use
`--solar-radio-flux-sfu` for an explicit override or `--f107-store` for a pinned
local dataset. See [F10.7 resolver](../nsb_components/airglow/f107-resolver.md).

CLI selection and override:

```bash
--components airglow
--solar-radio-flux-sfu 130
--airglow-vertical-profile profile.toml
```

When no value is supplied, NSB uses the documented default. Built-in site
profiles may select explicit planning assumptions, but the current CTAO profiles
do not include dedicated site-calibrated airglow continua. Selecting a geometry,
extinction model, or measured F10.7 does not upgrade scientific maturity.
See the [Airglow runtime guide](../nsb_components/airglow/README.md) and
[optical profile audit](../nsb_components/airglow/110-optical-vertical-profile-audit.md).

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
- There is no bundled experimental starlight fallback in `all`.

Downstream systems should record the returned model version and component list
instead of assuming that `all` always expands to a fixed number of components.

## Scientific maturity

Software readiness and scientific calibration are separate. A component may be
reliable as software while still being a generic model, planning preset,
published reference, proxy, or experimental product. Use
[Model maturity](../specifications/model-maturity.md), [Scientific metadata](../specifications/scientific-metadata.md),
and the [Validation matrix](../specifications/validation.md) when deciding whether a result is
appropriate for scientific or operational use.
