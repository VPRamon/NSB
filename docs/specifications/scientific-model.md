# Concepts and implementation

Status: Current overview of the runtime model.
Audience: Library users, CLI users, and new contributors.
Scope: Core quantities, component selection, evaluator behavior, and scientific
interpretation boundaries.
Non-goals: This guide is not a validation report or a release checklist.

Night-sky background is photon radiance arriving from diffuse and unresolved
sources. NSB reports the sum over 300–650 nm in photons per square centimetre per
nanosecond per steradian.

## Query inputs

- observer: geodetic longitude, latitude, and ellipsoidal height;
- time: typed UTC instant;
- target: ICRS/J2000 right ascension and declination;
- components: a bit mask selecting physical contributors.

Siderust supplies time scales, coordinates, ephemerides, atmosphere primitives,
events, and HEALPix. NSB supplies component composition, empirical NSB assets,
atmospheric-transport orchestration (`nsb::transport`), planning searches,
maturity metadata, and CLI presentation.

Atmospheric propagation is conceptually separate from source emission and from
future instrument response. The reusable transport foundation lives in
`nsb::transport` (identity and direct Beer–Lambert paths). Existing components
may still apply component-specific propagation until generic transport coverage is defined
explicitly; see [Atmospheric transport](atmospheric-transport.md).

## Components

Zodiacal light is sunlight scattered by interplanetary dust. Airglow is
atmospheric emission varying with season, time of night, solar activity, and
viewing geometry. Moonlight is lunar light scattered in the atmosphere.
Integrated starlight is unresolved catalogue-star flux mapped in Galactic
coordinates.

`ALL` includes Zodiacal, Airglow, Moonlight and the registered compact production
Starlight map. The exact runtime assets have conditional redistribution approval
under #103; this is separate from observational accuracy. Experimental spectral
maps and scattering remain opt-in and do not alter default predictions.
A validated external override
admits production metadata only after its map and provenance sidecar pass the
complete fail-closed contract.

## Point and window evaluation

`NsbEvaluator` loads immutable tables once. Point evaluation composes selected
components.

Window evaluation prepares a threshold context for the whole query:

```text
UTC query window
  -> TT/MJD search window
  -> Sun-altitude periods
  -> astronomical-night periods and airglow phase bins
  -> target-visible periods
  -> Moon-up periods when moonlight is selected
  -> candidate windows
  -> smooth subwindows split at phase and visibility boundaries
  -> adaptive exact-sample search with bracketed crossing refinement
```

This context avoids recomputing the same solar and lunar event information for
every sampled time. It does not replace the physical component models: accepted
threshold crossings are still evaluated against the exact zodiacal, airglow,
moonlight, and starlight component functions. Point evaluation keeps the
standalone component paths because it has no query-wide interval context to
reuse. If a subwindow is short or the sampled curve is too close to the
threshold for adaptive acceptance, the threshold search falls back locally to
the bounded scan path.

## Scientific interpretation

Maturity metadata is part of the result, not documentation-only text. Generic,
planning, proxy, published-reference, experimental, and calibrated meanings
must remain distinct. B/V values are central-wavelength diagnostics until a
validated passband integration replaces them.
