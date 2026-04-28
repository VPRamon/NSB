# Reimplementing darknsb in Rust with SideRust

> **Historical note**
> This feasibility report predates the simplified public API. It remains useful
> as design background, but the current crate no longer includes the Python
> compatibility layer, vendored `darknsb` sources, or the named-target catalog.
> See `../README.md` and `../examples/` for the current surface.

## Executive summary

Yes, `darknsb` could be reimplemented in Rust and substantially strengthened by the SideRust toolchain in `../`. SideRust is a strong fit for the astronomical geometry layer: time scales, observer sites, Sun/Moon ephemerides, topocentric horizontal coordinates, Moon phase, typed coordinate frames, and event searches. The sibling crates `qtty`, `affn`, `tempoch`, and `cheby` cover unit safety, affine coordinate semantics, precise time handling, and high-performance interpolation.

However, SideRust does not currently implement the photometric/radiative-transfer model itself. A Rust reimplementation would still need a dedicated NSB crate or module for spectra, photon-radiance units, zodiacal/starlight/airglow/moonlight components, data-table ingestion, interpolation, and validation against the Python/SkyCalc outputs.

## Sibling tools inspected

The parent `../` directory is the canonical Rust implementation layer and contains:

| Tool | Role relevant to darknsb |
|---|---|
| `siderust` | High-precision astronomy and satellite mechanics in Rust. Provides ephemerides, coordinate transforms, observatories, solar/lunar APIs, altitude/azimuth APIs, and optional JPL DE440/DE441 backends. |
| `qtty` | Strongly typed physical quantities and units. Useful for angles, time, lengths, areas, wavelengths, and explicit unit conversions. |
| `tempoch` | Typed astronomical time primitives with UTC/TT/UT1/TDB handling, leap seconds, Delta-T/EOP support, and high-precision split storage. |
| `affn` | Strongly typed affine geometry kernel: centers, frames, Cartesian/spherical/ellipsoidal coordinates, rotations, translations, and compile-time invalid-operation prevention. |
| `cheby` | Chebyshev interpolation toolkit with node generation, coefficient fitting, Clenshaw evaluation, and segment tables. Useful for fast smooth approximations and cached ephemeris/radiance tables. |
| `siderust/siderust-ffi` | C ABI bridge if the resulting model must be exposed to Python, C, or C++ adapters. |

The local `siderust` crate is version `0.6.1` and depends on `affn`, `cheby`, `qtty`, `tempoch`, `chrono`, `nalgebra`, and `wide`. Its default feature set is lightweight; optional `de440` and `de441` features enable JPL ephemeris backends.

## What SideRust can replace from darknsb

### Time and ephemerides

`darknsb` currently relies on Astropy `Time`, `get_sun`, `get_moon`, and manual MJD-based approximations. SideRust can replace this with:

- `tempoch`/`siderust::time` typed Julian Date and Modified Julian Date values.
- `Vsop87Ephemeris` as a default analytical backend.
- Optional `De440Ephemeris` or `De441Ephemeris` for higher-fidelity JPL-backed solar-system positions.
- Built-in generated VSOP87, ELP2000, and IERS EOP data for normal offline builds.

This would remove Python/Astropy runtime dependency and make time-scale choices explicit.

### Observatory geometry

`darknsb` uses Astropy site names:

```text
CTAO-N -> lapalma
CTAO-S -> paranal
```

SideRust already includes:

```text
ROQUE_DE_LOS_MUCHACHOS
EL_PARANAL
```

These map naturally to CTAO-N and CTAO-S style observing sites. They are stored as WGS84 geodetic coordinates with typed longitude, latitude, and height.

### Horizontal coordinates

The Python model depends on source altitude/azimuth, source zenith distance, Moon altitude, Moon zenith distance, and Moon-target angular separation. SideRust provides:

- topocentric Sun and Moon coordinates,
- `Moon::get_horizontal(...)`,
- `Sun::get_horizontal(...)`,
- altitude/azimuth APIs,
- explicit horizontal-coordinate convention documentation.

SideRust's native convention is standard astronomical azimuth: origin north, increasing clockwise through east. That aligns well with Astropy-style AltAz expectations, but a reimplementation should include a direct convention test because moon LUTs and SkyCalc tables may encode azimuth assumptions.

### Moon phase and illumination

`darknsb` computes:

```text
k = (1 + cos(i)) / 2
```

where `i` is the Moon phase angle. SideRust already exposes a lunar phase API with:

- geocentric and topocentric phase geometry,
- illuminated fraction,
- phase angle,
- elongation,
- waxing/waning classification,
- principal phase event search.

This can replace `GetMoonPhase`, `GetMoonPhaseAngle`, and parts of `get_new_moon.py`.

### Coordinate safety

`darknsb` uses Astropy units and frames dynamically. Rust with `siderust` + `affn` + `qtty` would make many categories of mistakes compile-time errors:

- mixing topocentric and geocentric coordinates,
- mixing horizontal and ecliptic frames,
- mixing angular and length quantities,
- treating geodetic height as radial distance,
- invalid affine operations such as position + position.

For a scientific model with many frame and unit conversions, this is a major reliability improvement.

## What SideRust does not yet provide

The following must be implemented in a new NSB layer:

| Needed by darknsb | Status in SideRust |
|---|---|
| Zodiacal-light brightness table from Leinert et al. | Not present. Preserve LUT |
| S10 photometric unit and B/V conversion constants | Not present. Implement units in qtty|
| Spectral radiance type such as `ph / (cm^2 ns sr nm)` | Not present as a domain abstraction. Implement units in qtty ?|
| Solar spectral irradiance/radiance table ingestion | Not present as an NSB model. |
| SkyCalc-derived starlight spectrum integration | Not present. |
| Airglow continuum/line model | Not present. |
| Rayleigh/Mie optical-depth and scattering model from Jones/Noll | Not present. |
| FITS table ingestion for SkyCalc outputs | Not present. |
| SIMBAD/name resolver equivalent | Not present and should probably remain outside core Rust. |
| End-to-end NSB component API | Not present. |

The conclusion is not "SideRust already has darknsb"; it is "SideRust can replace the astronomy foundation and make the new darknsb implementation safer, faster, and more maintainable."

## Proposed Rust architecture

A clean implementation would be a new crate, for example:

```text
darknsb-rs/
  Cargo.toml
  src/
    lib.rs
    request.rs
    result.rs
    geometry.rs
    spectra.rs
    integration.rs
    photometry.rs
    data/
      mod.rs
      solar.rs
      starlight.rs
      zodiacal_table.rs
      moon_tables.rs
      skycalc_fits.rs
    components/
      mod.rs
      zodiacal.rs
      starlight.rs
      airglow.rs
      moonlight.rs
      extinction.rs
      scattering.rs
```

### Public API shape

The public layer should make the scientific contract explicit:

```rust
pub struct NsbRequest {
    pub site: ObservingSite,
    pub epoch: TimeOrJulianDate,
    pub target: TargetDirection,
    pub wavelength_min_nm: f64,
    pub wavelength_max_nm: f64,
    pub include_moonlight: bool,
    pub atmosphere: AtmosphereProfile,
}

pub struct NsbResult {
    pub total: BandIntegratedPhotonRadiance,
    pub zodiacal: Option<ComponentResult>,
    pub starlight: Option<ComponentResult>,
    pub airglow: Option<ComponentResult>,
    pub moonlight: Option<ComponentResult>,
    pub b_mag_arcsec2: Option<f64>,
    pub v_mag_arcsec2: Option<f64>,
}
```

The active Python-compatible default should be:

```text
wavelength range = 300-650 nm
include_moonlight = false
components = zodiacal + starlight + active airglow polynomial
```

Then a fuller model can enable scattered moonlight and seasonal airglow explicitly.

### Geometry layer

Use SideRust for:

1. UTC input to typed astronomical epoch.
2. CTAO site to `Geodetic<ECEF>`.
3. Target ICRS direction or explicit horizontal direction.
4. Source altitude/azimuth and zenith distance.
5. Source ecliptic latitude/longitude for zodiacal light.
6. Sun ecliptic longitude.
7. Moon altitude, distance, phase angle, illuminated fraction, and Moon-target separation.

Potential gap: if SideRust lacks a one-call equivalent of Astropy's `source.heliocentrictrueecliptic` for arbitrary apparent target directions at a specific epoch, add a focused transform helper in SideRust rather than duplicating astronomy math in the NSB crate.

### Spectral layer

Implement a typed spectral container:

```text
Spectrum<XUnit, YUnit> {
    wavelengths,
    values,
}
```

Needed operations:

- interpolation onto another wavelength grid,
- nearest-index lookup for B/V filters,
- unit conversion,
- wavelength-bin integration,
- energy-radiance to photon-radiance conversion,
- spectrum multiplication by dimensionless correction curves.

`qtty` can carry many primitive units, but it may be worth adding small NSB-specific newtypes for:

```text
SpectralPhotonRadiance = ph / (cm^2 ns sr nm)
BandPhotonRadiance     = ph / (cm^2 ns sr)
SpectralEnergyRadiance = W / (m^2 um sr)
S10Brightness
```

Even if the internal arrays are `f64`, public APIs should use typed wrappers.

### Data layer

The Python model's data should be made explicit and testable:

| Python input | Rust strategy |
|---|---|
| `solar_spectrum.dat` | Parse at build time with `build.rs` or at runtime with a data registry. |
| `radiance_starlight.txt` | Parse into a static spectrum with provenance metadata. |
| Zodiacal table hardcoded in Python | Move to a documented static Rust table or external CSV/TOML. |
| `mie_m15s1.dat` and `sscatcor_m15s1.dat` | Parse into 2D interpolation grids. |
| FITS SkyCalc outputs | Either use a FITS reader crate or convert once into a simpler versioned table format. |
| Moon LUT CSVs | Use `csv` + `serde` or convert to compact binary/JSON at build time. |

For reproducibility, each data source should have:

- units,
- coordinate conventions,
- source citation,
- checksum,
- parse test,
- one small golden-value test.

### Interpolation and performance

Use ordinary bilinear interpolation for exact Python parity first. Use `cheby` later where it is scientifically justified:

- smooth solar/lunar spectra over wavelength,
- altitude-dependent airglow fits,
- precomputed Moon ephemeris or scattering factors,
- fast repeated all-sky maps.

The first Rust milestone should prioritize parity and traceability, not approximation speed. After parity is proven, Chebyshev segment tables can replace hot interpolation paths with measured tolerances.

## Component-by-component reimplementation plan

### 1. Python-compatible dark mode

This is the best first milestone because it matches active `get_NSB.py`.

Implement:

1. `CalculateSL` parity:
   - read starlight spectrum,
   - convert to photon radiance,
   - integrate 300-650 nm.
2. `CalculateAG` parity:
   - cubic altitude polynomial,
   - same constants,
   - same B/V S10 constants.
3. `CalculateZL` parity:
   - ecliptic target geometry,
   - Sun ecliptic longitude,
   - Leinert table interpolation,
   - solar spectrum scaling,
   - reddening correction,
   - extinction correction,
   - photon conversion and integration.
4. Result summation and B/V magnitude calculation.

This would produce a robust Rust equivalent of the current active dark-NSB behavior.

### 2. Replace fragile runtime lookups

Add explicit inputs:

- `ObservingSite::CtaoNorth` -> SideRust Roque de los Muchachos coordinates.
- `ObservingSite::CtaoSouth` -> SideRust El Paranal coordinates.
- target by RA/Dec or already-transformed direction.

If name resolution is needed, make it an optional adapter feature outside the deterministic core.

### 3. Add scattered moonlight

Port the inactive `CalculateMoon` stack only after dark-mode parity is locked down.

Key scientific pieces:

- Moon albedo versus phase angle,
- Moon top-of-atmosphere spectrum,
- Rayleigh/Mie optical depths,
- single-scattering integral through spherical atmosphere,
- Mie phase interpolation,
- multiple-scattering correction grid,
- ground reflectance,
- final photon-radiance integration.

This stage needs more validation because the Python code itself contains TBD comments, unexplained downsampling, and partial double-scattering support.

### 4. Add physical airglow model

The current active airglow is only a cubic fit versus altitude. A second model can use:

- `airglow_cont.dat`,
- season/month,
- solar radio flux,
- Van Rhijn correction,
- ozone/molecular transmission if required.

Keep both:

```text
AirglowModel::PythonPolynomial
AirglowModel::SkyCalcContinuum
```

so users can choose Python parity or improved physical behavior.

## Validation strategy

A scientifically credible Rust reimplementation should be validated at several levels.

| Level | Validation |
|---|---|
| Data parsing | Check row counts, wavelength ranges, units, and known sample values. |
| Geometry | Compare Sun/Moon altitude, Moon phase, ecliptic target coordinates, and Moon-target separation with Astropy for fixed cases. |
| Component parity | For fixed site/time/target cases, compare Rust ZL, SL, AG, and total against Python outputs. |
| Unit conversions | Test energy-to-photon conversion and arcsec^2-to-sr conversion independently. |
| Regression cases | Include CTAO-S/SgrA* example from `get_NSB.py`, CTAO-N examples, low/high altitude targets, near zodiacal table boundaries, and Moon near/full cases when moonlight is enabled. |
| Physical sanity | Check monotonicity where expected: airglow polynomial range, moonlight rising with phase/altitude and decreasing with Moon-target separation, extinction increasing near horizon. |
| Performance | Benchmark all-sky maps and repeated time-series calculations. |

## Engineering advantages of Rust + SideRust

| Advantage | Impact |
|---|---|
| Compile-time units and frames | Reduces silent dimensional and frame-mixing errors. |
| Deterministic data packaging | Avoids runtime dependency on Astropy site/name caches. |
| Strong errors | Replace `print`/sentinel/`exit()` behavior with typed `Result` errors. |
| Reusable library API | Expose core model to CLI, Python, C/C++, or services. |
| Performance | Eliminate Python loops in scattering and map calculations; enable parallelism safely. |
| Feature flags | Keep lightweight dark mode separate from heavy FITS/JPL/moonlight features. |
| FFI support | Existing SideRust FFI pattern can expose the model to downstream non-Rust tooling. |
| Testability | Golden tests can lock model behavior and data provenance. |

## Risks and decisions

| Risk/decision | Recommendation |
|---|---|
| Exact parity versus scientific modernization | Start with exact parity for active dark mode; add improved models behind explicit options. |
| SideRust AGPL and darknsb BSD-3-Clause | BSD-3-Clause material can generally be incorporated into AGPL projects if notices are preserved, but confirm project policy before copying code/data. |
| FITS dependency | Prefer build-time conversion to a simple checked format unless runtime FITS ingestion is a hard requirement. |
| Python code ambiguities | Treat comments like "TBD" and "can't remember why" as validation blockers, not implementation details to preserve blindly. |
| Modern SciPy incompatibility | Do not reproduce `interp2d`; implement explicit bilinear interpolation with known boundary behavior. |
| Target-name resolution | Keep outside the core model to preserve deterministic computation. |
| Ecliptic coordinate parity | Verify SideRust frame definitions against Astropy's `heliocentrictrueecliptic`; document any intentional differences. |

## Recommended implementation roadmap

1. Create a Rust NSB crate that depends on `siderust`, `qtty`, `tempoch`, `affn`, and optionally `cheby`.
2. Port only active dark-mode components first: zodiacal light, starlight, and polynomial airglow.
3. Add deterministic site and target APIs; avoid network lookups in the core.
4. Build data parsers and static tables with checksums.
5. Generate Python golden outputs for representative cases before changing the model.
6. Add Rust golden tests with component-level tolerances.
7. Add moonlight as a separate feature once dark parity is stable.
8. Add optional FFI/Python bindings only after the Rust API stabilizes.

## Final assessment

A Rust reimplementation is not only feasible; it is likely the better long-term engineering path. SideRust can provide the precise, typed astronomy backbone that `darknsb` currently delegates to Astropy, while the new NSB layer can focus on radiance physics, spectra, tables, and validation. The main work is not celestial mechanics; it is carefully porting and validating the photometric and atmospheric model.
