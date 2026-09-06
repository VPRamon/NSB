# Airglow generic baseline provenance audit

Status: Scientific provenance and Option D planning-proxy decision record
(originated as issue #108 audit). Not the canonical Airglow runtime guide — see
[../README.md](../README.md).

## Summary
This audit documents the complete **default Airglow computation pipeline** and the provenance/classification of every scientific/default assumption that the current implementation uses.

It also inspects whether any **implicit Paranal/CTAO/whitelist site dependence** exists when evaluating Airglow at arbitrary Earth locations.

**Verdict (Option D):** NSB exposes an arbitrary-location Airglow evaluator, but the empirical continuum is **Paranal-derived / Paranal-trained**. Outside Paranal it is an **explicit generic/planning proxy** with provenance and limitations exposed in metadata. It must **not** be described as a globally calibrated or globally validated scientific baseline. Geographically generic API ≠ globally calibrated dataset.

> Post-audit note (#110): the hard-coded viewing term described below has since
> become an explicit `AirglowGeometryModel`. The unchanged default is a 90 km
> Van Rhijn thin shell; caller-provided vertical emission profiles use the real
> observer altitude. This API change does not alter this audit's maturity verdict
> or turn any generic/planning result into a site calibration. See the
> [current runtime guide](../README.md).
>
> Post-audit note (#147): Airglow calibration and temporal domains are now
> valid-by-construction. Runtime code carries semantic `AirglowNightPhase` and
> `AirglowSeason` values, and validated correction tables have fixed `4 × 7`
> shape. The intentional unbounded-night fallback is represented explicitly as
> `AirglowNightPhase::FullNight`; malformed correction structure can no longer
> fall through to a neutral `1.0` correction.

## Scope (default pipeline)
Default Airglow computation is the path used by:
- `NsbEvaluator` airglow evaluation (default model composition) via `NsbModelConfig::generic_clear_sky()`, `NsbModelConfig::cta_n_planning()`, and `NsbModelConfig::cta_s_planning()`.
- `crates/nsb/src/components/airglow/` implementation modules:
  - `mod.rs`
  - `model.rs`
  - `calibration.rs`
  - `continuum.rs`
  - `temporal.rs`
  - `geometry.rs`
  - `output.rs`
  - `tests.rs`

The bundled baseline continuum is:
- `crates/nsb/data/airglow_cont.dat`
- registered in `crates/nsb/data/manifest.toml` (canonical scientific provenance)
- loaded by `crates/nsb/src/components/airglow/calibration.rs`
- surfaced at runtime via build-generated `assets::bundled_asset(...)` metadata → Airglow metadata (not independently copied provenance constants)

The Airglow scientific maturity/status surface that downstream users see is produced by:
- `crates/nsb/src/evaluator/metadata.rs` (`airglow_metadata`)

## Provenance classification categories (exactly one)
Every scientific/default assumption below is classified as exactly one of:
1. **derived from caller location/time/direction**
2. **Paranal-derived empirical baseline reused as generic/planning proxy** (not globally calibrated)
3. **site-specific calibration**
4. **convenience/default assumption**
5. **unresolved / insufficient provenance** (only where evidence truly cannot establish the fact)

For each classified item, the audit lists the **exact origin** (file, struct, parameter name, constant, data asset, or upstream source label).

## Default computation path (end-to-end)
Input:
- `observer: Geodetic<ECEF>` (caller location) and its fields `lon/lat/height`
- `time: Time<UTC>` (caller UTC instant)
- `target: SphericalDirection<EquatorialMeanJ2000>` (caller direction)
- Airglow model configuration:
  - `site_profile: SiteProfileId` from `NsbModelConfig`
  - `solar_radio_flux: SolarFluxUnits` from `NsbModelConfig`

### 1) Target altitude / viewing geometry
1.1. `altitude = target_altitude(time, location, target)`  
**Origin**: `crates/nsb/src/components/airglow/geometry.rs::target_altitude`  

1.2. `zenith = (90.0 - altitude).clamp(0.0, 90.0)`  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_night_phase`  

1.3. Altitude acceptance / failure mode  
If altitude is non-finite or `altitude <= -90.0`, Airglow returns zero outputs for the query.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_night_phase` checks `!alt.is_finite() || alt <= -90.0`.

### 2) Baseline continuum (Paranal-derived empirical template)
2.1. The built-in baseline continuum is loaded once per evaluator.  
**Origin**: `crates/nsb/src/evaluator/core.rs::NsbEvaluator::with_config` loads `airglow::load_builtin_standard()`.

2.2. `AirglowContinuum` is populated from the bundled file `crates/nsb/data/airglow_cont.dat`.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs::load_builtin_standard` sends `include_str!("../../../data/airglow_cont.dat")` through the same parser and validation boundary used by `AirglowContinuum::from_str`.

2.3. Built-in continuum byte integrity is compile-time pinned; scientific provenance is registry-derived.  
**Origin**:
- the build-time scientific asset validation pins the embedded bytes to the manifest SHA-256 `d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f`
- `assets::bundled_asset("airglow_cont.dat")` supplies schema/source/license/generator/calibration_status for metadata

### 3) Seasonal correction
3.1. `season = season(time, location)` returns an `AirglowSeason`.  
**Origin**: `crates/nsb/src/components/airglow/temporal.rs::season`.

3.2. Local-solar-month logic is computed from longitude.  
**Origin**: `temporal.rs::local_solar_datetime`, which computes:
- `offset_seconds = (location.lon.value() / 15.0 * 3600.0).round() as i64`
- and then `dt + offset_seconds`, where `dt` is derived from `time.to_chrono()`.

3.3. Monthly mapping to the six named “double-month” seasons.  
**Origin**: `temporal.rs::season` match:
- December / January → `AirglowSeason::DecJan`
- February / March → `AirglowSeason::FebMar`
- April / May → `AirglowSeason::AprMay`
- June / July → `AirglowSeason::JunJul`
- August / September → `AirglowSeason::AugSep`
- October / November → `AirglowSeason::OctNov`

`AirglowSeason::FullYear` is the explicit aggregate fallback when the UTC instant cannot be represented by `chrono`; it is not a structural table-lookup fallback.

3.4. Seasonal correction is a typed, infallible lookup in the validated calibration table.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_night_phase` calls `continuum.mean_correction(phase, season)`. `CorrectionTable` is validated once as a fixed `4 × 7` table, so no runtime bounds fallback is possible.

These seasonal/TON matrices are **inherited from the Paranal-trained continuum model**, not independently re-derived for the caller site.

### 4) Time-of-night correction
4.1. Airglow is only evaluated inside an “astronomical night” interval.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum` uses `night_phase(time, location)` and returns zero if it is `None`.

4.2. Astronomical-night classification uses hard-coded solar-altitude threshold -18°.  
**Origin**: `crates/nsb/src/components/airglow/temporal.rs::ASTRONOMICAL_TWILIGHT` and
`SunBody::below_threshold(..., ASTRONOMICAL_TWILIGHT, ...)`.

4.3. Search robustness parameters for bracketing the night interval.  
**Origin**: `temporal.rs` constants:
- `INITIAL_NIGHT_SEARCH_RADIUS = 2.0 days`
- `MAX_NIGHT_SEARCH_RADIUS = 200.0 days`
- `NIGHT_SEARCH_EXPANSION_FACTOR = 4.0`

4.4. Mapping within a classified astronomical night uses 3 equal semantic phases.  
**Origin**: `temporal.rs::airglow_phase_periods_for_window` splits `night.period` into thirds, and `temporal.rs::night_phase_from_night` maps:
- `[0, 1/3)` → `AirglowNightPhase::FirstThird`
- `[1/3, 2/3)` → `AirglowNightPhase::MiddleThird`
- `[2/3, 1]` → `AirglowNightPhase::LastThird`

4.5. A phase-unbounded astronomical night uses the explicit full-night calibration semantics.  
If the night is found but its phase is not bounded, the temporal model returns `AirglowNightPhase::FullNight`.  
**Origin**: `temporal.rs::airglow_phase_periods_for_window` and `temporal.rs::night_phase_from_night`.

### 5) Solar/F10.7 correction
5.1. Default solar radio flux (F10.7) is “neutralizing” for the bundled slope+const.  
**Origin**: `crates/nsb/src/components/airglow/units.rs::DEFAULT_SOLAR_RADIO_FLUX`.  
**Note**: Fixed convenience default; date-aware F10.7 resolution is tracked by #109 and is out of scope for this audit.

5.2. Solar-radio-flux validation and failure mode.  
If solar flux is non-finite or `<= 0`, Airglow returns zero.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_night_phase` calls `is_valid_solar_flux`.

5.3. Solar correction is linear in solar radio flux.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs::AirglowContinuum::solar_activity_correction`, which evaluates the validated intercept + slope × solar-radio-flux expression.

5.4. Coefficients are parsed from the bundled baseline file and validated as finite before `AirglowContinuum` can exist.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs` parses and validates `solar_activity_const` and `solar_activity_slope`.

These coefficients are part of the Paranal-trained continuum model (Noll/SkyCalc lineage).

### 6) Van Rhijn viewing-geometry correction (not atmospheric extinction)
6.1. The default geometry uses the validated baseline emission height to construct the Van Rhijn model.  
**Origin**: `crates/nsb/src/components/airglow/model.rs::with_shared_continuum` constructs `VanRhijnConfig::from_continuum_height(continuum.emission_height_km())`; the selected geometry supplies the scalar LOS factor during evaluation.

**Van Rhijn is a LOS / emitting-layer geometric path-length correction.** It is **not** atmospheric extinction (Rayleigh/Mie/molecular absorption).

6.2. Emission height is taken directly from the bundled baseline file and must be finite and greater than zero before calibration construction succeeds.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs` parsing/validation of the `height` block (label: “height (typical altitude of emission [km])”).

Alternative vertical-emission geometry is tracked by #110 and is out of scope here.

### 7) Site/profile scaling and atmospheric scattering (separation boundary)
7.1. The complete wavelength-dependent continuum expression before spectral
integration is:
`global_scale × solar_corr × seasonal_corr × Van Rhijn × Noll_scatter(λ) × user_scale`.
Scalar corrections and Van Rhijn/geometry form `scalar_scale`; `Noll_scatter(λ)`
is the wavelength-dependent Noll-2012 effective Rayleigh/Mie transmission applied
exactly once inside `integrate_attenuated_continuum` before the 300–650 nm
integral (not a second post-equation attenuation stage).
**Origin**: `crates/nsb/src/components/airglow/continuum.rs` and
`crates/nsb/src/components/airglow/extinction.rs`.

Van Rhijn remains LOS/emitting-layer geometry only. Noll scattering is a separate
atmospheric stage using `SiteProfile.atmosphere` pressure/Rayleigh/Mie inputs.
Molecular atmospheric absorption from the full ASM/SkyCalc pipeline is still not
reproduced (see § Remaining ASM gaps).

7.2. `user_scale` and `profile.atmosphere` are set from the active site profile.  
**Origin**: `crates/nsb/src/evaluator/core.rs` Airglow evaluation calls:
`Airglow::with_shared_continuum(...).with_atmosphere(profile.atmosphere).with_scale(profile.airglow.scale)`

7.3. Bundled Airglow profile scale provenance and calibration maturity are site-profile metadata.  
**Origin**: `crates/nsb/src/site.rs::AirglowSiteCalibration`:
- template path: `"NSB/data/airglow_cont.dat"`
- profile scale: `scale: ScaleFactors::new(1.0)` for built-ins
- provenance: Paranal-derived continuum reused as generic/planning proxy; neutral site scale; not site-calibrated
- assumptions: explicit “No CTAO-specific airglow continuum scale is bundled yet … instead of silently claiming a calibrated site airglow model.”

7.4. Built-in CTAO presets are explicitly planning presets (not calibrated).  
**Origin**: `crates/nsb/src/site.rs::SiteProfileId::profile` sets:
- `CalibrationStatus::PlanningPreset` for `CtaNorth` and `CtaSouth`

### 8) Spectral + integrated 300–650 nm results
8.1. Wavelength integration domain is hard-coded to 300–650 nm.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::{WL_LOW,WL_HIGH}` used by `integrate_attenuated_continuum`.

8.2. Central diagnostic “B” and “V” wavelengths are hard-coded (445 and 551 nm).  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::{B_FILTER,V_FILTER}`.

8.3. Integrated result uses spectrally attenuated baseline shape × scalar radiance scaling.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::integrate_attenuated_continuum`
then `integrated = integrated_relative_attenuated * radiance_scale`.

### 9) Scientific metadata / provenance surface
9.1. Airglow’s maturity classification in evaluator outputs is driven by `SiteProfileId`.  
**Origin**: `crates/nsb/src/evaluator/metadata.rs::airglow_metadata`, which calls:
`component_status_for_site_profile(site_profile)`

9.2. Component calibration status mapping for Airglow is:
- `GenericClearSky => GenericClearSky`
- `CtaNorth | CtaSouth => PlanningPreset`

**Origin**: `crates/nsb/src/evaluator/metadata.rs::component_status_for_site_profile`.

9.3. Provenance string is composed from:
- site-profile airglow provenance note
- site-profile name
- template identifier
- **registry-derived** baseline identity (path/schema/sha256/source/license/generator/calibration_status)
- explicit markers: Paranal/Noll/SkyCalc-derived baseline; `site_calibrated=false`

**Origin**: `crates/nsb/src/evaluator/metadata.rs::airglow_metadata` via `airglow::airglow_continuum_asset()`.

## Provenance classification (audit table)
The following list enumerates every scientific/default assumption used in the default pipeline and assigns it to exactly one category.

| # | Assumption / default input | Category | Exact origin |
|---|------------------------------|----------|---------------|
| 1 | Observer location `lon/lat/height` | (1) | caller `NsbEvaluator::evaluate` → `PointQuery.observer` → `airglow::Airglow` uses `self.location` |
| 2 | Observer longitude drives `AirglowSeason` | (1) | `temporal.rs::local_solar_datetime` uses `location.lon` |
| 3 | Observer location drives astronomical-night interval | (1) | `temporal.rs::astronomical_night_containing` uses `SunBody::below_threshold(&location, ...)` |
| 4 | Caller time `Time<UTC>` | (1) | `NsbEvaluator::evaluate` → `PointQuery.time` → `Airglow::compute` |
| 5 | Caller target direction `ra/dec` | (1) | `PointQuery.target` → `target_altitude(...)` |
| 6 | Altitude `target_altitude` | (1) | `geometry.rs::target_altitude` |
| 7 | Zenith angle is computed by clamped `90-altitude` | (1) | `continuum.rs::evaluate_continuum_with_night_phase` |
| 8 | Altitude hard acceptance threshold `altitude <= -90` | (4) | `continuum.rs::evaluate_continuum_with_night_phase` check |
| 9 | Solar-flux default value (neutralizing F10.7) | (4) | `components/airglow/units.rs::DEFAULT_SOLAR_RADIO_FLUX` (date-aware resolver: #109) |
| 10 | Solar-flux positivity requirement | (4) | `components/airglow/units.rs::is_valid_solar_flux` |
| 11 | Solar-correction linear form | (2) | `AirglowContinuum::solar_activity_correction` uses validated intercept/slope from Paranal-trained baseline |
| 12 | Solar-correction intercept/coefficients | (2) | `calibration.rs` parses and validates solar constants from `airglow_cont.dat` |
| 13 | Bundled continuum `global_scale` (~79.829) | (2) | Paranal-trained scale block in `airglow_cont.dat` |
| 14 | Bundled emission height for Van Rhijn | (2) | height block in `airglow_cont.dat` (typical 90 km) |
| 15 | Van Rhijn geometry factor (LOS / emitting-layer geometry; **not** extinction) | (4) | `AirglowGeometryModel::VanRhijn` / `VanRhijnConfig` (alt. geometry: #110) |
| 16 | Astronomical twilight threshold -18° defines night domain | (2) | `temporal.rs::ASTRONOMICAL_TWILIGHT` (matches upstream model convention) |
| 17 | Astronomical night is split into 3 equal semantic phases | (2) | `temporal.rs::night_phase_from_night` → `FirstThird` / `MiddleThird` / `LastThird` |
| 18 | Unbounded-phase fallback uses `AirglowNightPhase::FullNight` | (4) | `temporal.rs::night_phase_from_night` |
| 19 | Season mapping maps months into six named `AirglowSeason` variants | (1) | `temporal.rs::season` month mapping (caller longitude) |
| 20 | Seasonal/night-phase correction lookup | (2) | validated fixed `CorrectionTable` from Paranal-trained `mean` block in `airglow_cont.dat` |
| 21 | Baseline uncertainty correction table | (2) | validated fixed `CorrectionTable` from Paranal-trained `sig` block in `airglow_cont.dat` |
| 22 | Radiance scaling uses SkyCalc-native photon radiance unit | (4) | `units.rs::SkyCalcSpectralPhotonRadiance` |
| 23 | Wavelength integration domain 300–650 nm | (4) | `continuum.rs::{WL_LOW,WL_HIGH}` |
| 24 | Diagnostic B/V wavelengths 445/551 nm | (4) | `continuum.rs::{B_FILTER,V_FILTER}` |
| 25 | Site/profile scale `profile.airglow.scale` is applied multiplicatively | (3) | `evaluator/core.rs` Airglow evaluation uses `profile.airglow.scale` |
| 26 | Built-in profiles use a neutral site scale (1.0) | (4) | `site.rs::AirglowSiteCalibration::skycalc_neutral` sets `scale: 1.0` |
| 27 | Built-in CTAO profiles are declared `PlanningPreset`, not calibrated | (4) | `site.rs::SiteProfileId::profile` sets `CalibrationStatus::PlanningPreset` |
| 28 | Metadata classification for Airglow component uses `SiteProfileId` mapping | (4) | `evaluator/metadata.rs::component_status_for_site_profile` |
| 29 | Correction lookup is structurally infallible after construction | (4) | `calibration.rs::CorrectionTable`; dimensions and numeric entries validated before `AirglowContinuum` construction |
| 30 | Baseline template identity/checksum from asset registry + build-time byte validation | (2) | `manifest.toml` + generated asset metadata |
| 31 | Time/season/solar/Van Rhijn correction model domain limited to astronomical night | (2) | `continuum.rs` returns zero if `night_phase` is `None` |
| 33 | Baseline wavelength-resolved relative mean spectrum | (2) | Paranal/FORS1-derived relative continuum in `airglow_cont.dat` |
| 34 | Baseline wavelength-resolved relative uncertainty spectrum | (2) | relative_sigma column in `airglow_cont.dat` |
| 35 | Integrated relative 300–650 nm shape from baseline spectrum | (2) | `continuum.rs::integrate_attenuated_continuum` integrates validated spectrum over 300–650 nm |
| 36 | Integrated absolute uncertainty shape from baseline uncertainty spectrum | (2) | `continuum.rs::integrate_attenuated_continuum` integrates `|relative_sigma × transmission|` |
| 37 | B/V relative diagnostics from baseline spectrum at 445/551 nm | (2) | `continuum.rs::integrate_attenuated_continuum` linear interpolation |
| 38 | Relative uncertainty aggregation as quadrature of level and shape | (4) | `continuum.rs` `level.hypot(shape)` |
| 39 | Night phase uses UTC→TT conversion before phase/search | (1) | `temporal.rs::utc_time_to_tt_mjd` |
| 40 | Astronomical-night bracketing search parameters (adaptive window) | (4) | `temporal.rs` search-radius constants |
| 41 | Exact historical upstream file/release imported into NSB | (5) | not recorded in repo; lineage known (see § Baseline continuum audit) |
| 42 | Upstream redistribution/license terms | (5) | `manifest.toml` `license` explicitly unresolved |
| 43 | Noll-2012 effective Rayleigh/Mie airglow scattering (wavelength-dependent) | (2)/(4) | `extinction.rs` + Siderust `rayleigh_optical_depth_bodhaine99` / `mie_optical_depth`; uses `profile.atmosphere` |
| 44 | ASM molecular atmospheric absorption for airglow | **known unapplied limitation** | full Cerro Paranal ASM/SkyCalc molecular transmission dataset not bundled |

Notes:
- Category (2) for this asset means **Paranal-derived empirical model reused as a geographically generic planning proxy**, not “globally empirically calibrated”.
- Item (5) is reserved for facts the repository truly cannot establish (exact imported file/release; license). **Paranal origin is established** by literature and is not unresolved.

## Baseline continuum audit: `crates/nsb/data/airglow_cont.dat`

### KNOWN (established by literature + bundled asset evidence)

1. **Lineage: Noll et al. 2012 / Cerro Paranal Advanced Sky Model / ESO SkyCalc**  
   The numeric continuum (normalization near 0.543 µm, optical relative spectrum, seasonal/TON matrices, solar correction, emission height, global scale ~79.829) matches the airglow continuum of the Cerro Paranal Advanced Sky Model described in:
   - Noll et al. 2012, A&A 543 A92
   - ESO Cerro Paranal Advanced Sky Model / SkyCalc documentation  
     ([The Cerro Paranal Advanced Sky Model](https://www.eso.org/observing/etc/doc/skycalc/The_Cerro_Paranal_Advanced_Sky_Model.pdf), §§6.2.6–6.2.7)

2. **Observational derivation: FORS1 at Cerro Paranal**  
   ESO documentation states the optical residual airglow continuum was derived from 874 FORS1 spectra of the ESO sky-model verification dataset (Moon below horizon), after subtraction of other sky-model components, with reference flux level analysed at 0.543 µm. FORS1 setups covering the continuum windows span roughly **0.365–0.89 µm** (wavelengths ≲0.44 µm least covered).

3. **Bundled asset identity**  
   - Path: `airglow_cont.dat` / runtime label `NSB/data/airglow_cont.dat`
   - Schema: `skycalc-airglow-continuum-v1`
   - SHA-256: `d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f`
   - Canonical registry: `crates/nsb/data/manifest.toml`

4. **File-internal model parameters (present in the bundled `.dat`)**  
   - Normalization / adaptation at 0.543 µm (“constant shape”)
   - `scale = 79.829`
   - Seasonal (6 double-month) and time-of-night (3-phase, `alt_sun < -18`) matrices
   - Solar activity `cons`/`slope`
   - Emission height 90 km
   - Wavelength-dependent relative continuum + uncertainties
   - Header note: “Additional corrections: airmass and extinction”
   - Header note: “very uncertain at < 0.4 and > 0.9 mum”

5. **Absence of the literal string “Paranal” in the `.dat` is not evidence of site-independence.**  
   The continuum is a Paranal-derived/Paranal-trained empirical product reused outside Paranal as a planning approximation.

### UNKNOWN (must not invent)

1. **Exact historical source file / SkyCalc or ASM release** that was originally imported into NSB is not recorded.
2. **Upstream redistribution / license terms** are not established in the repository (`manifest.toml` records this explicitly). Do not invent a license.

### Remaining ASM gaps (known model limitations — not unresolved provenance)

Upstream Cerro Paranal Advanced Sky Model documentation also applies molecular
atmospheric absorption to airglow using a wavelength-dependent transmission
dataset (ASM §§6.2.6–6.2.7). NSB does **not** reproduce that molecular stage.

**NSB Airglow now applies** (since #114):
- Van Rhijn emitting-layer geometry (`siderust::atmosphere::van_rhijn_factor`)
- Noll-2012 effective Rayleigh/Mie scattering using `SiteProfile.atmosphere`
  pressure/Rayleigh/Mie assumptions and Siderust optical-depth kernels

**NSB Airglow stack (complete continuum expression before spectral integration):**
`global_scale × solar_corr × seasonal_corr × Van Rhijn × Noll_scatter(λ) × user_scale`

`Noll_scatter(λ)` is applied once spectrally; there is no second atmospheric-
scattering multiplication after that term.

The Noll `f_R`/`f_M` fits were derived primarily for `z ≲ 60°` (Noll §4.1). NSB
evaluates the same parametric form at larger zenith distances for numerical
stability, but those results are extrapolations with weaker upstream validation.

Therefore:
- Van Rhijn ≠ Noll scattering (separate modules and metadata)
- Full SkyCalc/ASM numerical parity is **still not claimed** while molecular
  absorption remains absent
- Generic and CTAO planning profiles remain planning assumptions, not calibrated

The bundled asset header still lists “Additional corrections: airmass and
extinction”; NSB now implements the Rayleigh/Mie scattering portion of that
correction. Molecular absorption remains an explicit documented gap.

### Spectral domain limitation (300–650 nm)

NSB integrates Airglow over **300–650 nm**. Optical empirical constraints from the FORS1-derived continuum are **not uniform** across that interval:

- FORS1 continuum windows begin near ~0.365 µm; coverage below ~0.44 µm is weaker
- The bundled file itself marks continuum as “very uncertain at < 0.4 … mum”
- Therefore **300–~365/400 nm is particularly weak/uncertain** relative to the optical reference region around 0.543 µm
- The **integrated 300–650 nm value inherits additional UV-end uncertainty**

No numeric UV-end uncertainty envelope is invented here beyond what the asset’s tabulated `relative_sigma` already carries. Spectral non-uniformity of evidence quality is a **known limitation**; if metadata cannot fully encode wavelength-dependent evidence quality, that limitation remains documented in this audit and in `validated_domain` text.

## Hidden site dependence audit (Paranal/CTAO whitelist risk)
The Airglow default path uses three location-dependent computations:
1. `target_altitude(...)` uses the caller `Observer` location and UTC time.
2. `season(...)` uses caller longitude to compute local solar date/month and returns an `AirglowSeason`.
3. `night_phase(...)` uses caller location to compute astronomical night intervals via Siderust solar-altitude events and returns an `AirglowNightPhase` while inside the calibration domain.

The Airglow evaluation now uses `profile.atmosphere` for Noll Rayleigh/Mie
scattering (`extinction.rs`) in addition to `profile.airglow.scale`.

Therefore:
- **Location is an input, not a whitelist.** Arbitrary valid terrestrial coordinates remain supported.
- There is **no silent Paranal/CTAO observatory whitelist** gating evaluation.
- However, the **continuum itself is Paranal-derived**. An arbitrary-location result is a **planning approximation based on that Paranal-trained continuum**, not a site-calibrated or globally validated prediction.
- CTAO named profiles remain **planning presets** with neutral airglow scale; they are not promoted to `Calibrated`.

Key distinction: **geographically generic API ≠ globally calibrated scientific dataset.**

## Audit outcome decision (A/B/C/D)
### Chosen outcome: D — explicit generic/planning proxy with fail-honest metadata

**Rejected Option A** (“current baseline is defensible generic”) because:
- Literature establishes the continuum as Cerro Paranal / FORS1 / Noll–SkyCalc derived
- API acceptance of arbitrary coordinates does **not** make the scientific dataset globally calibrated
- Absence of the word “Paranal” in the `.dat` is not evidence of geographic genericity

**Options B/C** (new global baseline or parameterized climatology) are **not** implemented in this phase; they remain future scientific paths.

**Option D means:**
1. Keep the Paranal-derived continuum as the shared reference template for arbitrary locations.
2. Label results as generic/planning (never site-calibrated merely because a named profile exists).
3. Expose provenance: Paranal / Noll / SkyCalc lineage, checksum, schema, unresolved license/exact release.
4. Document applicability limitations: UV-end weakness, missing ASM molecular absorption, planning-proxy uncertainty.
5. Preserve architecture so #38 can add optional site calibration on top without changing location-as-input.

Applicability domain (honest):
- Arbitrary Earth lon/lat/height remain first-class inputs
- Outputs are **planning/generic proxies**, not site calibrations
- CTAO presets remain planning presets
- Daytime / outside astronomical night → zero (contract preserved)
- Not a claim of global empirical validation across climates/altitudes

## #108 acceptance evidence checklist
| Criterion | Evidence in this audit / code |
|-----------|-------------------------------|
| Lineage | Noll 2012 / Cerro Paranal ASM / ESO SkyCalc (§ Baseline continuum audit) |
| Paranal origin | FORS1 Cerro Paranal derivation; established (not unresolved) |
| Unresolved items | Exact historical import file/release; license |
| Vars from caller location/time | altitude, `AirglowSeason` via longitude, astronomical night / `AirglowNightPhase` |
| Corrections inherited from Paranal-trained baseline | continuum shape, global_scale, solar coeffs, seasonal/TON matrices, emission height |
| Arbitrary coords supported | location-as-input; regression tests for non-Paranal locations |
| Outputs are planning/generic proxy | Option D; metadata `calibration_status` + `site_calibrated=false` |
| CTAO remain planning presets | `CalibrationStatus::PlanningPreset`; not Calibrated |
| Applicability domain | documented above |
| 300–650 / UV-end limitations | documented; file header + FORS1 coverage |
| Noll Rayleigh/Mie scattering | implemented (#114); uses `profile.atmosphere` |
| Missing molecular ASM absorption | documented as remaining upstream gap |
| Uncertainty limitations | tabulated sigmas + UV-end/non-uniform evidence quality |
| Recommended future path | replace/refine proxy with global/climatological baseline without changing location-as-input; #38 site calibration overlay |

## Relationship to other issues
- #109 (F10.7 resolver): Subsequently implemented with explicit/bundled offline resolution and structured provenance.
- #110 (alternative geometry model): Subsequently implemented with explicit Van Rhijn and caller-provided vertical-profile geometry; Van Rhijn remains the default.
- #38 (CTAO scientific calibration): Not modified; CTAO remain planning presets.
- #114 (effective Rayleigh/Mie airglow scattering): Implemented; Van Rhijn remains separate geometry.
- #147 (valid-by-construction calibration/domain model): Implemented with typed phase/season semantics, fixed correction-table shape, and a shared validated loading boundary.

## Recommended remediation (Phase 1 — implemented)
1. Reclassify outcome as **Option D** with Paranal-derived generic/planning-proxy semantics.
2. Strengthen scientific asset registry provenance (KNOWN vs UNKNOWN; FORS1/Paranal/Noll/SkyCalc).
3. Derive runtime Airglow scientific provenance from build-generated `assets::bundled_asset()` metadata (canonical `manifest.toml` interpreted at compile time).
4. Document Noll scattering, remaining molecular-absorption gap, UV-end domain limitations, and valid-by-construction calibration/domain semantics in audit + metadata.
5. Preserve / refine regression tests:
   - arbitrary Earth location uses generic path (no Paranal/CTAO required)
   - CTAO planning presets remain non-calibrated in metadata
   - baseline asset identity/checksum is deterministic and registry-consistent
   - location-dependent quantities depend on caller location
   - named site choice does not alter baseline-only parameters when scale is neutral
   - daytime returns zero without false calibration claims
   - metadata must not claim full upstream parity while molecular ASM absorption is absent
