# NSB GUI

`nsb-gui` is the native desktop frontend for the NSB scientific library. It is
implemented in Rust with `egui`/`eframe`; it does not embed a browser and does
not invoke the command-line interface.

## Run

```bash
cargo run --locked -p nsb-gui
```

The GUI is intentionally a workspace application crate (`publish = false`), so
GUI dependencies do not become dependencies or public features of the
publishable `nsb` library crate.

## Inputs

Only the active representation is shown in each section:

- **Observer:** explicit geodetic coordinates, an offline native coordinate-map
  picker, or a site from Siderust's built-in observatory catalogue.
- **Date & time:** local civil date/time with a fixed UTC offset, JD (TT), or
  MJD (TT).
- **Target:** ICRS/J2000 or horizontal coordinates. Representation format is a
  separate choice between sexagesimal and decimal degrees.

Horizontal target coordinates are interpreted as the local pointing at the
selected window start and are converted, through Siderust, to the equivalent
fixed J2000 direction required by NSB. They are not treated as a fixed
altitude/azimuth mount track throughout the entire window.

## Observing-window semantics

The GUI delegates observing-window calculation to `nsb::ThresholdQuery`. A
matching interval is a continuous period satisfying every enabled criterion:

- integrated 300–650 nm photon radiance at or below the configured maximum;
- optional Sun-altitude ceiling (default `-18°`);
- optional target-altitude floor (default `0°`).

The radiance threshold is expressed in NSB's authoritative planning unit,
`ph cm^-2 ns^-1 sr^-1`. B/V `mag/arcsec²` values are displayed as diagnostics,
not used as an invented planning threshold.

The planner can return zero, one, or multiple matching windows; the GUI keeps
all of them and does not invent a ranking rule called “best”.

The main chart and the “All criteria” timeline row are painted from the
same `ThresholdQuery` periods. The separate solar-state row uses Siderust solar
events to distinguish daytime, twilight (Sun between 0° and −18°), and
astronomical night. Editing inputs never silently relabels an existing result:
the GUI marks it stale until **Calculate** is run again.

## Component contributions

Component shares are computed only from each component's additive integrated
photon radiance divided by total integrated photon radiance. The GUI never
computes percentages from astronomical magnitudes, which are logarithmic.

## Computation model

Model evaluation runs on a background worker thread and returns through a
channel; egui's render/event thread remains responsive. **Calculate** is
disabled until the submitted job finishes, so rapid clicks cannot create
unbounded work and there can be no older overlapping result to overwrite a
newer one. Input changes made while a result is visible are detected against
the submitted snapshot and clearly mark that result as stale.

## Native map scope

The Map observer mode is deliberately offline and native: it is an interactive
equirectangular coordinate picker rendered with `egui`. It does not fetch map
tiles and does not embed web content. A richer native geographic basemap can be
added later without changing the observer-input abstraction.

## Intentional limitations

- Local time uses an explicit fixed UTC offset, not a daylight-saving timezone
  database. The offset entered by the user is authoritative.
- Horizontal input defines a fixed celestial direction at the search start; it
  does not model a continuously fixed altitude/azimuth mount track.
- The native map is a coordinate picker without geographic basemap tiles.
