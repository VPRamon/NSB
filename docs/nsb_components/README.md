# NSB components

Status: Current component documentation index.
Audience: Users, developers, maintainers, and scientific reviewers.
Scope: Physical meaning, calculation, inputs, generated assets, and scientific
boundaries of each runtime NSB contributor.

NSB models the ground-based night-sky background as the sum of four separately
reported contributors:

```text
total NSB = zodiacal light + integrated starlight + airglow + scattered moonlight
```

All integrated results use the NSB optical planning band, 300–650 nm, and are
reported as photon radiance in photons cm⁻² ns⁻¹ sr⁻¹. Component diagnostics may
also expose wavelength-resolved radiance or B/V reference values; those B/V
values are monochromatic diagnostics, not passband-integrated magnitudes.

## Component guides

| Component | Physical source | How it is provided at runtime |
| --- | --- | --- |
| [Zodiacal light](zodiacal/README.md) | Sunlight scattered by interplanetary dust | Leinert (1998) brightness grid, solar spectrum, reddening, and optional atmospheric extinction |
| [Integrated starlight](starlight/README.md) | Unresolved stellar flux | Galactic HEALPix map prepared and validated offline |
| [Airglow](airglow/README.md) | Upper-atmosphere emission | Bundled empirical continuum adjusted for geometry and observing conditions |
| [Scattered moonlight](moonlight/README.md) | Lunar light scattered in the atmosphere | Jones et al. (2013) spectral model or Krisciunas & Schaefer (1991) V-band reference |

## Shared runtime behaviour

The evaluator computes the selected contributors independently and returns both
their individual outputs and their sum. `ComponentMask::ALL` and the CLI value
`--components all` select the production-safe default. Starlight is included
only when a validated production asset is available; the experimental seed is
never selected implicitly.

Each result carries model maturity, provenance, validated-domain, and
uncertainty metadata. These are part of the scientific result: a numerically
valid calculation is not automatically a site-calibrated prediction.

For selection and CLI examples, see the [runtime component overview](../user-guide/components.md).
For cross-component units and evaluation behaviour, see [Concepts and
implementation](../specifications/scientific-model.md).
