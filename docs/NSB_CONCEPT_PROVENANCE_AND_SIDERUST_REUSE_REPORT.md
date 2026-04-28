# NSB concept provenance and SideRust reuse report

> **Historical note**
> This document captures provenance and reuse analysis from the original porting
> effort. Some rows refer to removed compatibility layers or Python-derived test
> scaffolding; treat those as historical context rather than the current crate
> layout. The current user-facing API is documented in `../README.md`.

## Purpose

This report maps the concepts used by the NSB crate to their source of
knowledge, then evaluates whether each concept should become a generic
SideRust ecosystem feature that this crate can later reuse.

The assessment distinguishes between:

- **literature-backed models**: equations or tables from published articles,
- **bundled data artifacts**: files inherited from `darknsb` or SkyCalc-style
  inputs,
- **implementation inheritance**: behavior copied for parity with the Python
  reference,
- **generic formulas/theorems**: unit conversions, interpolation, integration,
  geometry, and photometry rules,
- **empirical fits**: coefficients derived from data, where the derivation may
  or may not be fully documented.

## Executive summary

The best candidates for upstream generic SideRust/qtty support are units,
spectral containers, interpolation/integration helpers, photometric
conversions, airmass/extinction primitives, atmospheric scattering primitives,
and provenance-aware data tables. These are reusable beyond NSB and would make
this crate smaller and safer.

The NSB-specific tables and operational choices should stay in this crate:
Leinert zodiacal-brightness values, SkyCalc-derived starlight spectra, CTAO
moon LUTs, Python-parity airglow coefficients, and the default component sum
`zodiacal + starlight + airglow`. They can depend on generic SideRust building
blocks without becoming generic SideRust features themselves.

Several data artifacts need stronger provenance before they should be promoted
or treated as stable science data: `solar_spectrum.dat`, `o3trans.dat`,
`mie_m15s1.dat`, `sscatcor_m15s1.dat`, `data/lut_moon/*.csv`, and the airglow
altitude polynomial coefficients. They are useful for parity, but the current
repository does not record enough per-file metadata to know exactly who
generated them, with what assumptions, and from which version of SkyCalc or
upstream source.

## Concept inventory

| NSB concept | Current implementation | Source of knowledge | Provenance strength | Generic SideRust candidate? | Recommendation |
|---|---|---|---|---|---|
| Overall dark-NSB decomposition | `src/nsb.rs` sums selected components; active Python-compatible default is ZL + SL + AG. | `darknsb` `get_NSB.py` active path; ESO Advanced Cerro Paranal Sky Model lineage. | Strong for implementation behavior, medium for scientific completeness. | **No**, not as a core generic feature. | Keep orchestration in `nsb`; reuse generic astronomy/photometry/atmosphere primitives from SideRust. |
| Component mask / request / result API | `ObservationRequest`, `ComponentMask`, `NsbResult`, `NsbComponent`. | Engineering API design for the Rust port. | Strong as local design, not a scientific source. | **Maybe**, as a pattern only. | Keep NSB-specific API local; consider generic result metadata conventions later. |
| CTAO site mapping | `Site::Paranal` and `Site::LaPalma` map to SideRust observatory constants. | Astropy site usage in Python; SideRust observatory catalogue. | Strong. | **Already generic** in SideRust. | Continue using SideRust observatory constants; add CTAO aliases locally or upstream as named observatory aliases if broadly useful. |
| Time scale conversion | `tempoch::Time<UTC>` bridged to `siderust::time::JulianDate`. | SideRust/tempoch time APIs and astronomical time-scale definitions. | Strong. | **Already generic**, but bridge ergonomics could improve. | Add a reusable SideRust/tempoch conversion if this pattern repeats across crates. |
| Source direction handling | Built-in small source catalogue plus direct RA/Dec. | Python `setup_source` behavior; hardcoded known coordinates. | Medium; names and coordinates are local operational choices. | **No** for arbitrary names; **maybe** for typed target direction. | Keep name catalogue local or adapter-level. Reuse generic typed sky directions from SideRust. |
| Horizontal coordinates | `star_horizontal`, site geodetic coordinates, altitude/azimuth. | Spherical astronomy; SideRust coordinate APIs. | Strong. | **Already generic** in SideRust. | Keep upstream; NSB should not duplicate this math. |
| Ecliptic coordinates for target | Source direction transformed to `EclipticMeanJ2000`. | Spherical coordinate transforms; zodiacal model needs ecliptic geometry. | Strong for transform, medium for exact Astropy parity. | **Already generic**, with parity caveat. | Use SideRust transform; add validation against Astropy for the exact ecliptic frame expected by the Leinert zodiacal model. |
| Solar ecliptic longitude | `src/ephemeris/sun.rs` uses SideRust VSOP87 transform. | Celestial mechanics/VSOP87 via SideRust. | Strong. | **Already generic** in SideRust. | Keep only the NSB-facing helper locally unless a one-call solar ecliptic longitude API is desired upstream. |
| Moon position, separation, and illumination | `Moon::get_horizontal` and `Moon::phase_geocentric` used by the moonlight placeholder. | SideRust lunar APIs; Python used Astropy `get_moon`. | Strong for geometry; moonlight use not implemented. | **Already generic** in SideRust. | Keep in SideRust and validate topocentric/geocentric choices before enabling moonlight. |
| 300-650 nm integration band | Constants in components. | `darknsb`/Cherenkov-camera convention from Python globals. | Strong as parity behavior, not universal science. | **No**. | Keep as NSB default; expose configurable wavelength bounds later. |
| Spectrum container | `src/spectra/spectrum.rs` stores wavelength and flux arrays. | Generic numerical/scientific programming pattern; Python `Spectrum` helper. | Strong as engineering concept. | **Yes**. | Add a typed generic spectrum/table abstraction to SideRust or a sibling crate, parameterized by x/y units and interpolation policy. |
| Linear interpolation with endpoint clamping | `Spectrum::interp`; zodiacal table interpolation. | Numerical method plus Python `np.interp`/`interp1d` parity. | Strong. | **Yes**. | Provide reusable interpolation traits and explicit out-of-range policies upstream. |
| Trapezoidal spectral integration | `Spectrum::integrate_range` and `integrate::band_integral`. | Standard numerical quadrature; Python parity originally used wavelength-bin sums. | Strong. | **Yes**. | Upstream as a generic sampled-spectrum operation; NSB can choose parity-compatible or physically preferred integration policies. |
| Filter integration | `integrate::filter_integral`, `spectra::filters`. | Generic photometric bandpass operation. | Medium because current B/V filters are approximate. | **Yes**, but not with approximate filters as authoritative data. | Upstream the operation, not the current placeholder B/V passbands. Add exact, cited passband datasets separately. |
| Energy-to-photon conversion | `units::erg_to_photon` and inline ZL conversion using `5.03e7 * lambda_A`. | Photon energy relation `E = h c / lambda`, expressed in CGS/Angstrom units. | Strong. | **Yes**, likely in `qtty`. | Promote as a unit-safe conversion trait/function to avoid magic constants in component code. |
| `BandPhotonRadiance` | Local newtype for `ph / (cm^2 ns sr)`. | NSB output unit from Python and Cherenkov-camera convention. | Strong. | **Yes**, in `qtty` or a photometry/radiometry module. | Promote once naming and dimensions are stable. |
| `SpectralPhotonRadiance` | Local newtype for spectral photon radiance. | Radiometry dimensional analysis; Python unit conversions. | Strong. | **Yes**, in `qtty`. | Promote with explicit wavelength basis (`per nm` vs `per Angstrom`) encoded or documented. |
| `S10` brightness | Local `S10` newtype; Leinert table and B/V fluxes use S10. | Astronomical surface-brightness unit: 10th-magnitude stars per square degree. | Strong for concept; zero-point details need documentation. | **Yes**, likely photometry-focused qtty/SideRust feature. | Promote with conversions, references, and clear band dependence. |
| Surface brightness `mag / arcsec^2` | `SurfaceBrightness::from_band_flux` uses `27.78 - 2.5 log10(flux)`. | Python `get_NSB.py` convention; astronomical magnitude formula. | Medium: formula is standard, but `27.78` zero point is model-specific. | **Partly**. | Upstream the magnitude/surface-brightness type and generic log conversion; keep this zero point as NSB/S10 model configuration unless cited. |
| B/V S10 constants for starlight and airglow | Hardcoded component constants. | Python comments say values are calculated from SkyCalc spectrum after conversion. | Weak-to-medium; derivation is not reproduced. | **No** as constants. | Keep local for parity; add a derivation script or cited source before relying on them scientifically. |
| B/V central wavelengths | B = 445 nm, V = 551 nm in components; Gaussian-ish placeholder filters. | Python globals plus canonical Johnson-band approximations. | Medium. | **Yes** for passband infrastructure; **no** for placeholder curves. | Upstream filter/passband abstractions; replace placeholder filters with exact cited passbands when known. |
| Leinert zodiacal-light LUT | `src/data/leinert.rs` 37x19 S10 table. | Leinert 1998, "The 1997 reference of diffuse night sky brightness", A&AS 127, 1-99; transcribed from `darknsb`. | Strong literature citation; transcription should have checksum/golden tests. | **Maybe**, but probably not core SideRust. | Keep in `nsb` unless SideRust grows a `sky_brightness` data module. Add source citation, checksum, and table-boundary tests. |
| Zodiacal unmeasured-corner clamps | Max-value substitutions for near-Sun/low-latitude table gaps. | Python implementation says missing Leinert values are interpolated/filled and clamps to max values. | Medium; behavior is clear, scientific rationale is weak. | **No**. | Keep as explicit NSB compatibility policy and document the validity caveat. |
| Zodiacal bilinear interpolation | `leinert_lookup_s10`. | Numerical interpolation over the Leinert 5-degree grid. | Strong. | **Yes** as generic table interpolation, not the table itself. | Upstream a 2D gridded-table interpolator with boundary policy controls. |
| Zodiacal spectrum = scaled solar spectrum | `zodiacal::compute` scales solar irradiance to match 500 nm S10 value. | `darknsb`/SkyCalc/Noll lineage; Python `GetZodiacalLightSpectrum` states this assumption. | Medium-to-strong. | **No** as generic SideRust, **maybe** as reusable sky-brightness component. | Keep model in NSB; rely on upstream spectrum/unit primitives. |
| Zodiacal reddening | `reddening_factor`. | Leinert 1998, as cited in Python `GetZodicalReddening`. | Strong citation; implementation should be tested. | **No** for core SideRust. | Keep as NSB/zodiacal component logic; may later live in a sky-brightness model crate. |
| Zodiacal atmospheric extinction correction | `extinction_transmission` with Noll-style `fext` and aerosol wavelength law. | Noll et al. 2012 per Python docs; aerosol behavior also overlaps Patat-style Mie parameterization. | Medium-to-strong; exact constants should be traced to equations/tables. | **Maybe**. | Upstream generic extinction/airmass primitives; keep Noll-specific correction as an optional atmospheric model. |
| Solar spectrum data | `data/solar_spectrum.dat`, loaded by `spectra::solar`. | Bundled with `darknsb`; used by SkyCalc-derived model. Exact original solar spectrum source is not recorded in the local file header. | Weak-to-medium. | **Maybe**, only with provenance. | Keep local until source/version/license/checksum are recorded; then consider a generic solar-spectrum dataset module. |
| Starlight spectrum | `data/radiance_starlight.txt`, loaded by `spectra::starlight`; component integrates fixed spectrum. | `darknsb` README says average starlight spectrum taken from SkyCalc; Python cites Noll 2012 and Jones 2013. | Medium; SkyCalc source known, generation settings not recorded. | **No** as core SideRust. | Keep in NSB with provenance metadata; upstream only the loader/integration machinery. |
| Fixed starlight component | `components::starlight` has no direction/time dependence. | Python `CalculateSL` parity. | Strong for parity, weak as complete science model. | **No**. | Keep local and label as fixed SkyCalc-derived average. |
| Airglow altitude polynomial | `components::airglow` cubic coefficients in target altitude. | Python `airglow_param`; likely an empirical fit from SkyCalc/airglow exploration, but derivation is not documented in code. | Weak. | **No** as generic feature. | Keep local for Python parity only. Find/generate the fitting provenance before treating it as a reusable model. |
| Advanced airglow continuum table | `data/airglow_cont.dat`, loader parses global scale and relative mean profile. | ESO/SkyCalc/Noll-style airglow continuum metadata in file header; Python has incomplete `airglowcont`. | Medium. | **Maybe**, as optional atmospheric/airglow model. | Upstream generic parsing/table infrastructure; keep this specific dataset local until provenance/version are complete. |
| Van Rhijn / airglow geometry correction | Present in Python advanced airglow path, not active in Rust component. | Atmospheric emission-layer geometry from airglow modeling literature; Noll/SkyCalc lineage. | Medium. | **Yes**, if implemented generically. | Candidate for a generic atmospheric-emission geometry helper after citations and tests are added. |
| Rayleigh optical depth | `atmosphere::rayleigh::optical_depth`. | Rust doc cites Bodhaine et al. 1999; Python doc cites Liou 2002 for a different parameterization. | Mixed: concept strong, current parity needs review. | **Yes**. | Upstream Rayleigh models with named parameterizations and tests. Resolve Bodhaine-vs-Liou mismatch explicitly. |
| Rayleigh phase function | `rayleigh::phase`; Python moon scattering uses Jones et al. 2013 equation 13 form. | Electromagnetic scattering theory / Jones 2013 implementation. | Strong. | **Yes**. | Upstream as a generic atmospheric scattering primitive. |
| Mie/aerosol optical depth | `atmosphere::mie::optical_depth` uses `tau0 * (lambda/550)^alpha`. | Python cites Patat 2011 and applies a short-wavelength cut; Rust doc references the Python parameterization. | Medium. | **Yes**, with named models. | Upstream aerosol optical-depth models with explicit parameter sets; keep Paranal defaults configurable. |
| Mie phase table | `data/mie_m15s1.dat`; loader TODO. | `darknsb` README says Cerro Paranal aerosol Mie grid valid for CTAO-S; ESO Sky Model lineage. | Medium; generation details absent. | **Maybe**, data local; interpolation generic. | Upstream 2D table interpolation and phase-function trait; keep this specific table in NSB unless provenance is completed. |
| Ozone transmittance | `data/o3trans.dat`, loader present. | Bundled SkyCalc-style data; no direct source in file header. | Weak-to-medium. | **Maybe**, only with provenance. | Keep local until source/version/units are documented; upstream generic transmittance spectrum handling. |
| Total optical depth / transmission | `atmosphere::extinction` sums Rayleigh + Mie and applies `exp(-airmass * tau)`. | Beer-Lambert attenuation plus atmospheric model components. | Strong. | **Yes**. | Good generic SideRust atmospheric primitive once unit types and model selection are added. |
| Airmass formulas | `geometry::airmass` implements several formulas. | Python comments cite Young & Irvine 1967, Young 1994, Krisciunas & Schaefer 1991, and Rozenberg 1966. | Strong concept; implementation labels should be audited against citations. | **Yes**. | Upstream with citation-backed enum variants and tests at zenith/horizon regimes. |
| Moon top-of-atmosphere spectrum | Present in Python; Rust moonlight currently returns zero. | Jones et al. 2013 equation 1: solar spectrum times lunar albedo, solid angle, and distance scaling. | Strong. | **Maybe**. | Geometry and radiometry pieces are generic; full moonlight model should remain optional/domain-specific. |
| Lunar albedo phase model | Python `GetMoonAlbedo`; Rust TODO. | Jones et al. 2013 scattered moonlight model. | Strong citation, implementation not ported. | **Maybe**. | Upstream only if SideRust wants a reusable lunar photometry module; otherwise keep in NSB moonlight. |
| Moonlight single-scattering integral | Python `scat_moon`; Rust TODO modules for single scatter/corrections. | Jones et al. 2013 equations and ESO Sky Model assumptions. | Medium-to-strong, but Python has TBD comments. | **Maybe**, as atmospheric radiative-transfer primitives. | Upstream low-level scattering geometry/phase functions; keep full model in NSB until validated. |
| Multiple-scattering correction grid | `data/sscatcor_m15s1.dat`; Rust parser TODO. | Bundled `darknsb` data; file header says multiple Rayleigh/Mie correction factors. | Medium; generation details absent. | **No** for data, **yes** for grid interpolation. | Keep data local; require provenance/checksum and golden values before use. |
| Moon LUT CSVs | `data/lut_moon/Phase_*_waxing_moon_LUT.csv`. | Precomputed darknsb moonlight tables; likely generated from the Python/ESO model for a season, but exact generation script/settings are not recorded here. | Weak-to-medium. | **No** as generic SideRust. | Keep local as operational NSB acceleration data; add metadata, checksums, coordinate conventions, and generator documentation. |
| Atmospheric shell constants for moon scattering | Python constants such as `HMAX`, `NPT_SCAT`, `LNSCALE`, aerosol single-scattering albedo. | Jones 2013/Noll/SkyCalc lineage plus Python implementation choices. | Medium. | **Maybe** for configurable atmosphere profiles, not fixed constants. | Upstream typed atmosphere profile structs; keep CTAO/SkyCalc defaults local. |
| Observatory pressure defaults | `Site::reference_pressure_hpa`; Python has Paranal and La Palma pressure constants. | Python `NSB_Utils.py` constants. | Medium. | **Maybe**. | Upstream generic site atmosphere metadata only if maintained broadly; otherwise keep NSB defaults local and configurable. |
| Golden fixtures and discrepancy report | `tests/golden` and `target/nsb_discrepancy_report.md`. | Captured Python `darknsb` outputs. | Strong for parity, not independent validation. | **No**. | Keep local; add independent SkyCalc/literature validation later. |
| Python bindings | `src/pybind`. | Engineering adapter choice. | Strong local design. | **No**. | Keep in NSB; reuse SideRust FFI patterns if they stabilize. |

## Upstream reuse backlog

The most valuable upstream work, ordered by likely reuse and low scientific
risk, is:

1. **Quantity types in `qtty`**: `S10`, spectral photon radiance, band photon
   radiance, surface brightness, and energy-to-photon conversion.
   *(Status: done in `qtty` `0.6.2-dev` — see `qtty/CHANGELOG.md`.
   `radiometry` feature exposes `S10`, `Radiance`, `SpectralRadiance`,
   `PhotonRadiance`, `SpectralPhotonRadiance`, and a typed `erg_to_photon`.
   `SurfaceBrightness` remains NSB-local for now because of its
   logarithmic, zero-point-dependent definition.)*
2. **Generic sampled spectra**: typed wavelength grids, interpolation policies,
   band integration, filter integration, and provenance metadata.
   *(Status: done in `siderust` `0.6.2-dev` under the optional `spectra`
   feature — see `siderust/CHANGELOG.md`. Exposes
   `SampledSpectrum<X, Y, S>` with strict-monotonic validation,
   `Interpolation`/`OutOfRange` policies, trapezoidal `integrate` /
   `integrate_range` / `integrate_weighted` returning `Quantity<Prod<Y, X>>`,
   `Provenance` / `DataSource`, and a generic two-column ASCII loader.
   Untyped `f64` numerical kernels under `spectra::algo` preserve NSB's
   historical `numpy.interp` parity bit-for-bit; NSB now delegates its
   `Spectrum::{interp, integrate, integrate_range}` and `filter_integral`
   to those kernels. The B/V Gaussian filter placeholders were removed
   per this report's recommendation, and `flux_to_mag`'s 27.78 zero-point
   relocated to NSB-local `nsb::photometry`.)*
3. **Generic gridded tables**: 1D/2D interpolation with explicit boundary
   behavior, checksums, row/column units, and data-source metadata.
   *(Status: done in `siderust` `0.6.2-dev` under the optional `tables`
   feature — see `siderust/CHANGELOG.md`. Exposes `Grid1D<X, V>` and
   `Grid2D<X, Y, V>` with strict-monotonic axis validation and per-axis
   `OutOfRange` policy, plus untyped `tables::algo::{linear_1d,
   bilinear, bilinear_unit}` `f64` kernels. NSB's
   `leinert_lookup_s10` now delegates its bilinear arithmetic to
   `tables::algo::bilinear_unit` while keeping its own corner-clamp and
   wrapped-axis indexing policy locally; bit-for-bit golden parity is
   preserved. Checksums and data-source metadata for bundled tables
   remain a separate provenance pass.)*
4. **Airmass and atmospheric transmission**: named formulas, Beer-Lambert
   transmission, Rayleigh/Mie/ozone model traits, and configurable site
   atmosphere profiles.
   *(Status: done in `siderust` `0.6.2-dev` under the optional
   `atmosphere` feature — see `siderust/CHANGELOG.md`. Exposes
   `airmass(zenith: Radians, AirmassFormula::{PlaneParallel,
   Young1994, Rozenberg1966, KrisciunasSchaefer1991})`,
   `rayleigh_optical_depth_bodhaine99`, `rayleigh_phase`,
   `MieParams { tau0, alpha, lambda_ref }` with a `MieParams::PARANAL`
   preset, and Beer-Lambert `transmission(tau, airmass)`. NSB now
   delegates `geometry::airmass` and `atmosphere::{rayleigh, mie,
   extinction}` to those upstreams while preserving its `f64`-only
   wrapper signatures; `cross_validate_against_python_golden` stays
   green. Ozone transmittance datasets and multi-layer profiles remain
   NSB-local for now.)*
5. **Photometry helpers**: magnitude/surface-brightness types, zero-point
   handling, and cited passband datasets.
6. **Lunar/atmospheric scattering primitives**: phase functions, aerosol
   profiles, shell integration geometry, and correction-grid interpolation.

## Concepts that should stay NSB-local

These are too model-specific, operational, or under-documented to be generic
SideRust features now:

- the active dark-NSB component mix and default 300-650 nm band,
- the Leinert zodiacal table and missing-corner clamp policy,
- the fixed SkyCalc-derived starlight spectrum,
- the current airglow altitude polynomial,
- CTAO-specific moon LUTs,
- Python-parity B/V S10 constants,
- golden fixtures captured from `darknsb`.

## Provenance gaps to close

Before the model is presented as scientifically traceable rather than only
Python-compatible, add metadata for every bundled data artifact:

| Artifact | Missing metadata |
|---|---|
| `data/solar_spectrum.dat` | Original source, version/date, units confirmation, license, checksum. |
| `data/radiance_starlight.txt` | SkyCalc version, site/atmosphere settings, extraction method, checksum. |
| `data/airglow_cont.dat` | SkyCalc/source version, paper/table mapping, season/time-bin definitions, checksum. |
| `data/o3trans.dat` | Source model, atmospheric assumptions, units, checksum. |
| `data/mie_m15s1.dat` | Aerosol model source, grid axes, valid site/season/range, checksum. |
| `data/sscatcor_m15s1.dat` | Generator/model, grid axes, valid conditions, checksum. |
| `data/lut_moon/*.csv` | Generator script, SkyCalc/darknsb version, season, coordinate convention, phase definition, checksum. |
| Airglow polynomial coefficients | Fitting dataset, fitting script/notebook, residuals, validity range. |
| B/V S10 constants | Derivation from spectra/passbands, exact zero points, validation values. |

## Bottom line

SideRust should provide the generic astronomy, units, spectra, photometry,
interpolation, and atmosphere primitives. The NSB crate should keep the
scientific component models and datasets until they are both broadly reusable
and fully traceable. This separation lets NSB gain type safety and reuse without
turning SideRust into a repository of partially documented, CTAO-specific sky
brightness data.
