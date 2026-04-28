# NSB staged implementation plan

> **Historical note**
> This staged plan was written before the crate was simplified to the current
> `PointQuery` / `ThresholdQuery` API and `nsb` CLI. It is kept as background
> design context, not as the source of truth for the current implementation.

## Goal

Define a staged implementation path for a new NSB tool such that:

1. the public API is already useful from the first stage,
2. each subsequent stage improves physical fidelity without breaking the API,
3. early stages depend only on the most generic astronomy blocks,
4. later stages add the direction-dependent and spectrum-dependent pieces that make the model scientifically accurate.

The intended end product is a tool that returns:

```text
NSB(site, time, target, wavelength_range, model_options) -> total + component breakdown
```

with the default broadband target matching the current Python convention:

```text
300-650 nm
ph / (cm^2 ns sr)
```

---

## Guiding principle

The tool should **not** be implemented as "all physics or nothing".

Instead, it should follow this rule:

> keep one stable request/response interface, and progressively replace coarse approximations with more physical components.

That means:

- Stage 1 already returns a valid NSB estimate.
- Stage 2 keeps the same output shape, but improves the sky-condition logic.
- Stage 3 keeps the same output shape, but moves from scalar heuristics to spectral integration.
- Later stages refine individual physical components one by one.

---

## Stable target API from the beginning

Every stage should support the same high-level contract.

## Request

```text
site
time
target
wavelength range
model switches
```

Suggested fields:

| Field | Meaning |
|---|---|
| `site` | Observatory location or named preset such as CTAO-N / CTAO-S |
| `time` | UTC input, converted internally to astronomical time |
| `target` | RA/Dec or already-resolved sky direction |
| `wavelength_min_nm`, `wavelength_max_nm` | Integration band |
| `include_moonlight` | Whether lunar contribution is included |
| `model_level` | Approximation level or selected stage |

## Response

```text
total_nsb
component breakdown
geometry metadata
quality / stage metadata
```

Suggested fields:

| Field | Meaning |
|---|---|
| `total` | Total integrated NSB |
| `components.sun` | Informational geometry/condition block, not usually additive to dark NSB |
| `components.moonlight` | Lunar contribution if enabled |
| `components.zodiacal` | Zodiacal-light contribution |
| `components.starlight` | Starlight contribution |
| `components.airglow` | Airglow contribution |
| `geometry` | Sun altitude, Moon altitude, Moon phase, Moon-target separation, target altitude, ecliptic coordinates |
| `stage` | Which fidelity stage produced the result |
| `warnings` | Missing physics, out-of-range input, fallback behavior |

---

## Staged roadmap

## Stage 0 - Astronomical geometry foundation

### Purpose

Build the generic astronomy backbone before any NSB physics.

### Inputs used

- site
- time
- target

### Physics/data used

- Sun position
- Moon position
- Moon phase / illumination
- target horizontal coordinates
- target ecliptic coordinates

### Outputs added

- target altitude / azimuth / zenith distance
- Sun altitude
- Moon altitude
- Moon-target angular separation
- Moon illuminated fraction
- target ecliptic latitude and longitude
- Sun ecliptic longitude

### Why this stage matters

Every later NSB component depends on this layer:

- airglow needs target altitude,
- zodiacal light needs target ecliptic geometry relative to the Sun,
- moonlight needs Moon altitude, phase, distance, and separation from the target.

### Validity

This stage is **not yet an NSB estimator**, but it is the common foundation for every later stage.

---

## Stage 1 - Minimal valid NSB tool: dark-sky floor

### Purpose

Produce a first valid NSB estimate with a constant or site-dependent dark-sky baseline.

### Inputs used

- site
- time
- target

### Model

```text
NSB_total = constant_dark_floor
```

Optionally:

```text
NSB_total = site_specific_dark_floor
```

### Outputs added

- first valid `total`
- stage label
- geometry metadata from Stage 0

### Why this stage is useful

Even this crude stage already gives:

- a valid tool shape,
- an executable pipeline,
- integration tests for API, units, and site/time/target handling,
- a baseline against which every later stage can be compared.

### Scientific meaning

This stage is not physically complete, but it is operationally useful as a conservative first-order dark-sky estimate.

---

## Stage 2 - Sun/Moon condition-aware coarse NSB

### Purpose

Use the most generic sky-condition drivers before adding detailed spectra.

### Inputs used

- target altitude
- Sun altitude
- Moon altitude
- Moon phase
- Moon-target separation

### Model

Use simple heuristic penalties or regimes:

- daylight / twilight / dark-night classification from Sun altitude,
- rough moon penalty from:
  - Moon above horizon,
  - illumination fraction,
  - Moon-target separation.

Example coarse logic:

```text
NSB_total =
    dark_floor
  + coarse_sun_term(sun_altitude)
  + coarse_moon_term(moon_altitude, moon_phase, moon_target_sep)
```

### Outputs added

- valid all-sky rough estimator
- first explicit Moon contribution
- first explicit "observing condition" behavior

### Why this stage matters

This is the earliest stage where the tool starts behaving like a sky-brightness estimator rather than a constant.

### Scientific meaning

Still low fidelity, but already captures the two dominant generic drivers:

- solar contamination,
- lunar contamination.

---

## Stage 3 - Spectral backbone

### Purpose

Move from scalar brightness heuristics to a spectrum-aware pipeline.

### Inputs used

- wavelength grid
- integration bounds

### Model additions

Introduce:

- a spectral container,
- interpolation on wavelength grids,
- unit-aware conversions,
- integration from spectral radiance to band-integrated NSB.

### First required data

- solar spectral irradiance / radiance table

### Outputs added

- `spectra.*` internal capability
- broadband result from spectral integration rather than only scalar formulas

### Why this stage matters

This is the boundary between:

- a condition classifier,
- and a physically composable radiative model.

Without this stage, zodiacal light and moonlight can only be approximated crudely.

---

## Stage 4 - Airglow first-order model

### Purpose

Add the simplest physically motivated dark-sky component that is already present in the Python code.

### Inputs used

- target altitude

### Model

Use the current empirical altitude polynomial:

```text
airglow = f(target_altitude)
```

### Outputs added

- `components.airglow`
- first direction-dependent natural-sky term

### Why this stage matters

Airglow is part of the dark NSB even when the Moon is absent. This stage makes the tool useful for moonless conditions.

### Scientific meaning

This is a low-order empirical model, but it is directly compatible with the current active Python behavior.

---

## Stage 5 - Starlight fixed spectral term

### Purpose

Add the second active dark component from the Python code with minimal model risk.

### Inputs used

- starlight spectrum table
- wavelength integration

### Model

```text
starlight = integrate( fixed_scattered_starlight_spectrum )
```

### Outputs added

- `components.starlight`

### Why this stage matters

This stage is straightforward and stable:

- one data file,
- one conversion path,
- one band integration.

### Scientific meaning

Still coarse because it is not sky-position dependent, but it is already consistent with the active Python implementation.

---

## Stage 6 - Dark-mode parity milestone

### Purpose

Reach parity with the active `get_NSB.py` behavior before implementing the more difficult components.

### Model

```text
dark_nsb =
    airglow
  + starlight
  + zodiacal_light_coarse_or_placeholder
```

If zodiacal light is not yet present, this stage can still ship as:

```text
dark_nsb_partial =
    airglow
  + starlight
```

but the preferred target is:

```text
dark_nsb =
    airglow
  + starlight
  + zodiacal_light
```

### Why this stage matters

This is the first milestone where the Rust tool can replace the current Python "dark NSB" workflow for normal moonless use.

---

## Stage 7 - Zodiacal-light geometry

### Purpose

Implement the geometric part of zodiacal light before full spectral correction.

### Inputs used

- target ecliptic latitude `beta`
- target-Sun ecliptic longitude separation `Delta_lambda`

### Model

Use the Leinert-style brightness table at 500 nm:

```text
ZL_500 = LUT(beta, Delta_lambda)
```

### Outputs added

- `components.zodiacal.reference_500nm`
- first truly direction-sensitive dark-sky component

### Why this stage matters

This is the first stage where the model meaningfully uses the target's ecliptic position relative to the Sun.

### Scientific meaning

This is already much more physical than a global dark-floor model, even before reddening and extinction are added.

---

## Stage 8 - Full zodiacal-light spectral model

### Purpose

Upgrade zodiacal light from a 500 nm scalar table to the physically useful broadband component.

### Inputs used

- solar spectrum
- zodiacal-light 500 nm reference
- reddening law
- target zenith distance
- atmospheric extinction model

### Model

```text
zodiacal_spectrum(lambda) =
    solar_spectrum(lambda)
  * normalization_from_ZL_500
  * reddening(lambda, geometry)
  * extinction(lambda, target_zenith)
```

then:

```text
zodiacal = integrate(zodiacal_spectrum, 300-650 nm)
```

### Outputs added

- full `components.zodiacal`
- major dark-mode accuracy improvement

### Why this stage matters

This is the first component that is both:

- strongly directional,
- and explicitly spectrum-aware.

In practice, this is one of the most important accuracy upgrades for a moonless NSB tool.

---

## Stage 9 - Approximate moonlight from LUTs

### Purpose

Add a practical lunar contribution before porting the full scattering physics.

### Inputs used

- Moon altitude
- Moon azimuth
- Moon phase
- target altitude
- target azimuth

### Model

Use precomputed moonlight lookup tables when available:

```text
moonlight = LUT(moon_alt, moon_az, moon_phase, target_alt, target_az)
```

### Outputs added

- usable `components.moonlight`
- valid non-dark NSB mode

### Why this stage matters

This stage provides a fast and operationally useful moonlight estimate without immediately porting the hardest radiative-transfer code.

### Scientific meaning

More empirical than the final moon model, but very valuable as an intermediate deployable step.

---

## Stage 10 - Physical moonlight model

### Purpose

Replace or complement LUT-based moonlight with the full physics-inspired model present in the Python source.

### Inputs used

- Moon phase angle
- Moon distance
- Moon altitude
- Moon-target separation
- solar spectrum
- Moon albedo model
- Rayleigh optical depth
- Mie optical depth
- Mie phase grid
- scattering correction grids

### Model decomposition

1. Moon geometry
2. Moon top-of-atmosphere spectrum
3. atmospheric scattering into line of sight
4. transmission and correction factors
5. band integration

### Outputs added

- physics-based `components.moonlight`
- much better fidelity across Moon phase, altitude, and geometry

### Why this stage matters

This is the main step that turns the tool from a dark-NSB estimator into a broader natural-sky-brightness estimator.

---

## Stage 11 - Advanced airglow model

### Purpose

Replace the simple altitude polynomial with the more physical continuum model.

### Inputs used

- month / season
- solar radio flux
- target zenith distance
- site altitude
- airglow continuum tables
- Van Rhijn factor

### Model

Use `airglow_cont.dat` style modeling:

- seasonal scaling,
- solar-activity scaling,
- geometric Van Rhijn correction,
- spectral continuum integration.

### Outputs added

- improved `components.airglow`
- separation between:
  - `airglow_simple`
  - `airglow_physical`

### Why this stage matters

Airglow is one of the dominant dark-sky terms, so once dark-mode parity is achieved, this is one of the best places to improve scientific realism.

---

## Stage 12 - Atmospheric refinement layer

### Purpose

Refine component transport through the atmosphere consistently.

### Additions

- ozone transmission,
- molecular absorption where relevant,
- configurable pressure / aerosol assumptions,
- explicit site atmospheric profiles,
- clearer separation of:
  - outside-atmosphere spectrum,
  - extinction,
  - scattering into the line of sight.

### Why this stage matters

At this stage the model becomes a real engineering/science tool rather than a faithful port of the current simplified active path.

---

## Stage 13 - Calibration and validation stage

### Purpose

Turn the implementation into a trustworthy scientific product.

### Required work

- golden tests against the current Python outputs,
- cross-checks against SkyCalc where possible,
- unit tests for every data parser,
- geometry cross-checks against Astropy or known reference values,
- documented validity ranges,
- out-of-range behavior,
- numerical tolerance policy.

### Why this stage matters

This is what makes later high-fidelity stages credible rather than just more complicated.

---

## Dependency graph

The recommended implementation order is:

```text
Stage 0  -> geometry foundation
Stage 1  -> constant dark floor
Stage 2  -> coarse Sun/Moon-aware estimator
Stage 3  -> spectral backbone
Stage 4  -> simple airglow
Stage 5  -> fixed starlight
Stage 7  -> zodiacal geometry
Stage 8  -> full zodiacal-light spectral model
Stage 6  -> dark-mode parity milestone
Stage 9  -> moonlight LUT mode
Stage 10 -> physical moonlight
Stage 11 -> advanced airglow
Stage 12 -> atmospheric refinement
Stage 13 -> calibration and validation
```

Note: Stage 6 is a milestone rather than a technical dependency node. It is achieved once Stages 4, 5, and 8 are complete.

---

## What counts as "valid" at each stage

| Stage | Tool is valid for | Not yet valid for |
|---|---|---|
| 1 | constant dark-sky planning baseline | real directional sky brightness |
| 2 | rough observing-condition awareness | accurate dark NSB physics |
| 3 | spectral pipeline correctness | full physical NSB |
| 4 | first dark-sky directional term | full dark-sky realism |
| 5 | stable dark-sky additive term | spatially varying starlight |
| 6 | practical dark-NSB replacement | bright-Moon conditions |
| 8 | realistic moonless dark-NSB | Moon-contaminated conditions |
| 9 | practical Moon-on estimate | full lunar radiative transfer |
| 10 | full natural-sky brightness estimator | fully calibrated site-specific atmosphere |
| 11-13 | research/engineering-grade model | only limited by data quality and validation coverage |

---

## Recommended first delivery target

The best first meaningful delivery is:

```text
Stage 0 + Stage 3 + Stage 4 + Stage 5 + Stage 8
```

That combination gives:

- proper astronomy geometry,
- proper spectral integration,
- simple airglow,
- fixed starlight,
- full zodiacal light,
- a realistic moonless dark-NSB tool,
- parity with the active Python scope.

After that, the next highest-value upgrade is:

```text
Stage 9 or Stage 10
```

depending on whether the priority is:

- fast deployable Moon support, or
- physically faithful Moon support.

---

## Final recommendation

Build the new NSB tool in layers:

1. **geometry first,**
2. **then a valid coarse NSB estimator,**
3. **then spectral infrastructure,**
4. **then dark-sky physical components,**
5. **then lunar physics,**
6. **then atmospheric and calibration refinements.**

That sequence preserves usefulness from the start, avoids an all-or-nothing port, and creates a clean path from a simple but working tool to a scientifically defensible NSB model.
