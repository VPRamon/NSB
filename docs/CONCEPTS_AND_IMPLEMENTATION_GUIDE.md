# NSB Concepts and Implementation Guide

This guide is for readers who are new to astronomy and want to understand what
NSB computes today, which pieces are mature, and which pieces are still explicit
planning or development baselines.

## What "NSB" means

`NSB` stands for **night-sky background**: the light from the sky itself that is
present even when you are not looking directly at a star or galaxy.

For a ground-based observatory, that background matters because it acts like
"noise" on top of the object you want to observe.

In this crate, the result is mainly expressed as:

- `ph/(cm² ns sr)`: photons arriving per square centimeter, per nanosecond, per
  steradian. This is a **linear physical brightness**.
- `mag/arcsec²`: astronomical surface brightness in magnitudes per square
  arcsecond. This is the more traditional observer-facing brightness scale.

## What the crate answers

The crate supports two kinds of questions:

- `PointQuery`: "How bright is the sky at this exact time, from this observing
  site, in this target direction?"
- `ThresholdQuery`: "During this time window, when is the sky darker than some
  chosen limit?"

Those two query shapes are evaluated by `NsbEvaluator`.

## Model maturity and preset names

`NsbEvaluator::new()` uses `NsbModelConfig::generic_clear_sky()`. This is the
current default because it is evaluable with bundled runtime data and is explicit
about its scope: it is a generic clear-sky development/planning baseline, not a
validated CTAO production-science preset.

The API deliberately does not expose public `standard()` or `best_science()`
model presets. Those names are reserved for a future configuration whose inputs
are complete, reproducible, provenance-recorded, and quantitatively validated.

`NsbModelConfig::python_parity()` is hidden and reserved for historical
regression tests against the older Python implementation. It selects legacy model
choices and should not be used as a current science preset.

The default component set, `ComponentMask::ALL`, currently means zodiacal light,
airglow, and scattered moonlight. It intentionally excludes unresolved Galactic
starlight because no catalogue-derived bundled starlight map has been generated
and validated yet. Requests that select `ComponentMask::STARLIGHT` require an
explicit `StarlightModel::with_map(...)` configuration or the future bundled
catalogue-derived map.

## The basic astronomy words used here

You do not need deep astronomy knowledge to use the crate, but these terms appear
often:

- `location`: where the observer is on Earth.
- `target`: the direction in the sky you care about.
- `RA` / `right ascension`: the sky equivalent of longitude.
- `Dec` / `declination`: the sky equivalent of latitude.
- `altitude`: how high the target is above the local horizon. `0°` means on the
  horizon, `90°` means straight overhead.
- `zenith distance`: angular distance from straight overhead. It is
  `90° - altitude`.
- `ecliptic`: a Sun-centered sky coordinate system tied to the plane of Earth's
  orbit. It is useful for zodiacal light because zodiacal dust is concentrated
  near that plane.
- `geodetic coordinates`: latitude, longitude, and height of the observing site
  on Earth.

## What is implemented

The total NSB is modeled as a sum of four components:

- `zodiacal`
- `starlight`
- `airglow`
- `moon`

Each one lives in `crates/nsb/src/components/`.

## 1. Zodiacal light

Plain-language meaning: sunlight scattered by dust in the Solar System.

Why it matters: it can make the sky noticeably brighter, especially near the
ecliptic and in directions that are not far from the Sun's position on the sky.

How this crate models it:

- It converts the target direction from equatorial coordinates into ecliptic
  coordinates.
- It computes the angular difference between the target and the Sun in ecliptic
  longitude.
- It looks up a baseline brightness from the embedded Leinert (1998) table.
- It scales a solar spectrum to match that baseline brightness.
- It applies reddening and atmospheric extinction corrections.
- It integrates the resulting spectrum over the crate's wavelength band.

Public constructor: `ZodiacalLight::leinert1998()`.

Important intuition: zodiacal light depends strongly on **where you look relative
to the Sun and the ecliptic plane**.

## 2. Integrated starlight

Plain-language meaning: the combined glow of many unresolved stars.

Why it matters: even when you are not pointing at a bright star, countless faint
stars add a small background glow.

How this crate models it:

- It converts the target direction from equatorial coordinates into Galactic
  longitude and latitude.
- It looks up the target in a rectangular Galactic starlight map.
- It interpolates the map values and returns integrated radiance plus B/V S10
  summaries.
- The catalogue-derived bundled map is not available yet. Until a real map is
  generated from a recorded catalogue pipeline, bundled, and validated,
  `Starlight::catalogue_galactic_model()` reports `DataMissing`.

Public configuration path:

- `StarlightModel::with_map(...)` for explicit caller-supplied maps.
- `StarlightModel::BundledCatalogueMap` for the future bundled map, once it
  exists and is validated.

Important intuition: starlight is **target-direction dependent** because
unresolved stars are strongly concentrated toward the Galactic plane.

## 3. Airglow

Plain-language meaning: light emitted by Earth's upper atmosphere.

Why it matters: the night sky is not perfectly dark even with no Moon and no
clouds because the atmosphere itself emits light.

How this crate models it:

- It takes the target altitude above the horizon.
- It uses the bundled empirical continuum calibration.
- It applies Van Rhijn geometry and solar-activity, seasonal, and time-of-night
  corrections.

Public constructor: `Airglow::standard_clear_sky(...)`.

Scientific maturity note: this remains a site-bound empirical clear-sky model,
not a complete physical airglow forecast model.

## 4. Scattered moonlight

Plain-language meaning: moonlight scattered through the atmosphere into the
target direction.

Why it matters: when the Moon is up, it can dominate the sky brightness.

How this crate models it:

- It computes the Moon's position in the local sky.
- It computes the angular separation between the Moon and the target.
- It uses lunar phase geometry from `siderust`.
- It evaluates either the Jones et al. (2013) wavelength-resolved scattered
  moonlight model or the Krisciunas & Schaefer (1991) analytic V-band model.

Default generic clear-sky configuration uses `MoonlightModel::Jones2013Spectral`.
`MoonlightModel::KrisciunasSchaefer1991` is retained for explicit legacy use and
historical Python parity.

## What `NsbEvaluator` does internally

For a point evaluation, `NsbEvaluator::evaluate` does this:

1. Accept the observing site as geodetic coordinates.
2. Convert the target direction into the coordinate systems needed by the
   components.
3. Compute the target's local altitude where needed.
4. Evaluate each enabled component.
5. Sum the component radiances.
6. Report the total plus per-component contributions.

For a threshold-window search, `NsbEvaluator::periods_below_threshold` does this:

1. Start with the requested UTC time window.
2. Optionally keep only times when the Sun is below a chosen altitude. The
   default is `-18°`, which means astronomical twilight or darker.
3. Optionally keep only times when the target is above a chosen altitude. The
   default is `0°`, which means above the geometric horizon.
4. Scan only the surviving sub-windows.
5. Refine crossings of the threshold and return the dark intervals.

This is an optimization: it avoids spending full NSB evaluations on times that
are obviously unusable.

## How to read the inputs

`Observer` is a `Geodetic<ECEF>` location: longitude, latitude, and height.
Named-site parsing lives in the CLI crate, not in the runtime library.

`Target` is an equatorial sky direction in `RA/Dec`, using the
`EquatorialMeanJ2000` frame. In practice, you can treat that as: "give the
target's standard catalog sky coordinates."

## How to read the outputs

`NsbResult` contains:

- `integrated`: the total linear radiance over the model band.
- `b_mag` and `v_mag`: B-band and V-band surface brightness summaries.
- `components`: one entry per enabled component, so you can see what dominates.

## What is simple vs. detailed in the current model

More detailed parts:

- zodiacal light geometry and spectral scaling
- Moon position and phase handling
- event-driven threshold-window search

Simpler or pending parts:

- catalogue-derived starlight data generation and validation is still pending
- airglow remains an empirical clear-sky continuum model
- generic clear-sky atmospheric assumptions should be replaced by site-calibrated
  profiles for precision science use

That distinction matters because some outputs are more physically structured than
others.

## Module map

- `crates/nsb/src/lib.rs`: public exports
- `crates/nsb/src/evaluator.rs`: main API and orchestration
- `crates/nsb/src/components/zodiacal/`: zodiacal-light model
- `crates/nsb/src/components/starlight/`: integrated starlight model
- `crates/nsb/src/components/airglow/`: airglow model
- `crates/nsb/src/components/moonlight/`: scattered moonlight model
- `crates/nsb-cli/`: command-line interface

## If you only remember one mental model

The crate answers:

"At this place, at this time, in this direction, how bright is the sky
background?"

It does that by adding together explicitly selected, maturity-labelled
components: sunlight from Solar-System dust, unresolved starlight when explicitly
configured, glow from Earth's atmosphere, and scattered moonlight.
