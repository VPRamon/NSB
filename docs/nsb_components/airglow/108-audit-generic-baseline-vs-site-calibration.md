# Audit: Airglow generic baseline vs site calibration (Issue #108)

## Summary
This audit documents the complete **default Airglow computation pipeline** and the provenance/classification of every scientific/default assumption that the current implementation uses.

It also inspects whether any **implicit Paranal/CTAO/whitelist site dependence** exists when evaluating Airglow at arbitrary Earth locations.

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
- declared/validated by `crates/nsb/src/components/airglow/calibration.rs` and referenced in `crates/nsb/data/manifest.toml`

The Airglow scientific maturity/status surface that downstream users see is produced by:
- `crates/nsb/src/evaluator/metadata.rs` (`airglow_metadata`)

## Provenance classification categories (exactly one)
Every scientific/default assumption below is classified as exactly one of:
1. **derived from caller location/time/direction**
2. **generic empirical baseline**
3. **site-specific calibration**
4. **convenience/default assumption**
5. **unresolved / insufficient provenance**

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
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_time_bin`  

1.3. Altitude acceptance / failure mode  
If altitude is non-finite or `altitude <= -90.0`, Airglow returns zero outputs for the query.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_time_bin` checks `!alt.is_finite() || alt <= -90.0`.

### 2) Baseline continuum (global empirical template)
2.1. The built-in baseline continuum is loaded once per evaluator.  
**Origin**: `crates/nsb/src/evaluator/core.rs::NsbEvaluator::with_config` loads `airglow::load_builtin_standard()`.

2.2. `AirglowContinuum` is populated from the bundled file `crates/nsb/data/airglow_cont.dat`.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs::load_builtin_standard` parses `include_str!("../../../data/airglow_cont.dat")`.

2.3. Built-in continuum checksums are compile-time pinned.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs` uses `siderust::assert_data_checksum!` with:
- asset path: `NSB/data/airglow_cont.dat`
- checksum: `d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f`

### 3) Seasonal correction
3.1. `season_bin = season_bin(time, location)`  
**Origin**: `crates/nsb/src/components/airglow/temporal.rs::season_bin`.

3.2. Local-solar-month logic is computed from longitude.  
**Origin**: `temporal.rs::local_solar_datetime`, which computes:
- `offset_seconds = (location.lon.value() / 15.0 * 3600.0).round() as i64`
- and then `dt + offset_seconds`, where `dt` is derived from `time.to_chrono()`.

3.3. Monthly mapping to 6 “double-month” seasons.  
**Origin**: `temporal.rs::season_bin` match:
- `12 | 1 => 1`
- `2 | 3 => 2`
- `4 | 5 => 3`
- `6 | 7 => 4`
- `8 | 9 => 5`
- `10 | 11 => 6`

3.4. Seasonal correction term is read from the baseline matrix.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_time_bin`:
`continuum.mean_corrections[time_bin][season_bin]`, with fallback `1.0` if missing.

### 4) Time-of-night correction
4.1. Airglow is only evaluated inside an “astronomical night” interval.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum` uses:
- `time_of_night_bin(time, location)` and returns zero if it is `None`.

4.2. Astronomical-night classification uses hard-coded solar-altitude threshold -18°.  
**Origin**: `crates/nsb/src/components/airglow/temporal.rs::ASTRONOMICAL_TWILIGHT` and
`SunBody::below_threshold(..., ASTRONOMICAL_TWILIGHT, ...)`.

4.3. Search robustness parameters for bracketing the night interval.  
**Origin**: `temporal.rs` constants:
- `INITIAL_NIGHT_SEARCH_RADIUS = 2.0 days`
- `MAX_NIGHT_SEARCH_RADIUS = 200.0 days`
- `NIGHT_SEARCH_EXPANSION_FACTOR = 4.0`

4.4. Mapping within a classified astronomical night uses 3 equal phases.  
**Origin**: `temporal.rs::airglow_phase_periods_for_window` splits `night.period` into thirds,
and `temporal.rs::time_of_night_bin_from_night` maps:
- `[0, 1/3) => 1`
- `[1/3, 2/3) => 2`
- `[2/3, 1] => 3`

4.5. Phase unbounded fallback uses “row 0 full-night correction”.  
If the night is found but its phase is not bounded, `time_bin = 0`.  
**Origin**: `temporal.rs::airglow_phase_periods_for_window` and
`temporal.rs::time_of_night_bin_from_night` returns `Some(0)` when `!night.phase_bounded`.

### 5) Solar/F10.7 correction
5.1. Default solar radio flux (F10.7) is “neutralizing” for the bundled slope+const.  
**Origin**: `crates/nsb/src/components/airglow/units.rs::DEFAULT_SOLAR_RADIO_FLUX`.

5.2. Solar-radio-flux validation and failure mode.  
If solar flux is non-finite or `<= 0`, Airglow returns zero.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::is_valid_solar_flux` called from
`evaluate_continuum_with_time_bin`.

5.3. Solar correction is linear in solar radio flux.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_time_bin`:
`solar_corr = continuum.solar_activity_const + continuum.solar_activity_slope * solar_radio_flux`.

5.4. Coefficients are parsed from the bundled baseline file.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs::load_builtin_standard` parses:
- `solar_activity_const`
- `solar_activity_slope`

### 6) Van Rhijn viewing-geometry correction
6.1. The code computes Van Rhijn geometry from zenith angle and baseline emission height.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_time_bin`:
`van_rhijn_factor(Degrees::new(zenith).to::<Radian>(), continuum.emission_height_km)`.

6.2. Emission height is taken directly from the bundled baseline file.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs::load_builtin_standard`:
`emission_height_km` parsed from the `height` block (label: “height (typical altitude of emission [km])”).

### 7) Site/profile scaling (separation boundary)
7.1. The baseline continuum is global; site-specific contribution is only a multiplicative scale.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs` uses:
`scale = continuum.global_scale * solar_corr * seasonal_corr * van_rhijn * user_scale`

7.2. `user_scale` is set from the active site profile (or caller scale).  
**Origin**: `crates/nsb/src/evaluator/core.rs::evaluate_airglow` calls:
`Airglow::with_shared_continuum(...).with_scale(profile.airglow.scale)`

7.3. Bundled Airglow profile scale provenance and calibration maturity are site-profile metadata.  
**Origin**: `crates/nsb/src/site.rs::AirglowSiteCalibration`:
- template path: `"NSB/data/airglow_cont.dat"`
- profile scale: `scale: ScaleFactors::new(1.0)` for built-ins
- provenance: `provenance = "Bundled SkyCalc-derived empirical continuum template; neutral site scale."`
- assumptions: explicit “No CTAO-specific airglow continuum scale is bundled yet … instead of silently claiming a calibrated site airglow model.”

7.4. Built-in CTAO presets are explicitly planning presets (not calibrated).  
**Origin**: `crates/nsb/src/site.rs::SiteProfileId::profile` sets:
- `CalibrationStatus::PlanningPreset` for `CtaNorth` and `CtaSouth`

### 8) Spectral + integrated 300–650 nm results
8.1. Wavelength integration domain is hard-coded to 300–650 nm.  
**Origin**: `crates/nsb/src/components/airglow/calibration.rs::WL_LOW_NM` and `WL_HIGH_NM`,
used to compute `integrated_relative_300_650`.

8.2. Central diagnostic “B” and “V” wavelengths are hard-coded (445 and 551 nm).  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::{B_FILTER,V_FILTER}`.

8.3. Integrated result uses baseline shape × radiance scaling.  
**Origin**: `crates/nsb/src/components/airglow/continuum.rs::evaluate_continuum_with_time_bin`:
`integrated = integrated_relative_300_650 * radiance_scale`

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

**Origin**: `crates/nsb/src/evaluator/metadata.rs::airglow_metadata`.

## Provenance classification (audit table)
The following list enumerates every scientific/default assumption used in the default pipeline and assigns it to exactly one category.

| # | Assumption / default input | Category | Exact origin |
|---|------------------------------|----------|---------------|
| 1 | Observer location `lon/lat/height` | (1) | caller `NsbEvaluator::evaluate` → `PointQuery.observer` → `airglow::Airglow` uses `self.location` |
| 2 | Observer longitude drives `season_bin` | (1) | `temporal.rs::local_solar_datetime` uses `location.lon` |
| 3 | Observer location drives astronomical-night interval | (1) | `temporal.rs::astronomical_night_containing` uses `SunBody::below_threshold(&location, ...)` |
| 4 | Caller time `Time<UTC>` | (1) | `NsbEvaluator::evaluate` → `PointQuery.time` → `Airglow::compute` |
| 5 | Caller target direction `ra/dec` | (1) | `PointQuery.target` → `target_altitude(...)` |
| 6 | Altitude `target_altitude` | (1) | `geometry.rs::target_altitude` |
| 7 | Zenith angle is computed by clamped `90-altitude` | (1) | `continuum.rs::evaluate_continuum_with_time_bin` |
| 8 | Altitude hard acceptance threshold `altitude <= -90` | (4) | `continuum.rs::evaluate_continuum_with_time_bin` check |
| 9 | Solar-flux default value (neutralizing F10.7) | (4) | `components/airglow/units.rs::DEFAULT_SOLAR_RADIO_FLUX` |
| 10 | Solar-flux positivity requirement | (4) | `components/airglow/continuum.rs::is_valid_solar_flux` |
| 11 | Solar-correction linear form | (2) | `continuum.rs::evaluate_continuum_with_time_bin` uses `solar_activity_const/slope` from bundled baseline |
| 12 | Solar-correction intercept/coefficients | (2) | `calibration.rs::load_builtin_standard` parses solar constants |
| 13 | Bundled continuum `global_scale` | (2) | `calibration.rs::load_builtin_standard` parses `scale` block |
| 14 | Bundled emission height for Van Rhijn | (2) | `calibration.rs::load_builtin_standard` parses `height` block |
| 15 | Van Rhijn geometry factor is computed from zenith + emission height | (4) | `siderust::atmosphere::van_rhijn_factor(...)` |
| 16 | Astronomical twilight threshold -18° defines night domain | (2) | `temporal.rs::ASTRONOMICAL_TWILIGHT` |
| 17 | Time binning splits the astronomical night into 3 equal phases | (2) | `temporal.rs::time_of_night_bin_from_night` |
| 18 | Unbounded-phase fallback uses `time_bin=0` full-night row | (4) | `temporal.rs::time_of_night_bin_from_night` |
| 19 | Season binning maps months into 6 bins | (1) | `temporal.rs::season_bin` month mapping |
| 20 | Seasonal correction lookup (time_bin, season_bin matrix) | (2) | `AirglowContinuum.mean_corrections` parsed from `airglow_cont.dat` |
| 21 | Baseline uncertainty matrices | (2) | `AirglowContinuum.sigma_corrections` parsed from `airglow_cont.dat` |
| 22 | Radiance scaling uses SkyCalc-native photon radiance unit | (4) | `units.rs::SkyCalcSpectralPhotonRadiance` and `SkyCalcSpectralPhotonRadiance::new(scale)` |
| 23 | Wavelength integration domain 300–650 nm | (4) | `calibration.rs::{WL_LOW_NM,WL_HIGH_NM}` |
| 24 | Diagnostic B/V wavelengths 445/551 nm | (4) | `continuum.rs::{B_FILTER,V_FILTER}` |
| 25 | Site/profile scale `profile.airglow.scale` is applied multiplicatively | (3) | `evaluator/core.rs::evaluate_airglow` uses `profile.airglow.scale` |
| 26 | Built-in profiles use a neutral site scale (1.0) | (4) | `site.rs::AirglowSiteCalibration::skycalc_neutral` sets `scale: 1.0` |
| 27 | Built-in CTAO profiles are declared `PlanningPreset`, not calibrated | (4) | `site.rs::SiteProfileId::profile` sets `CalibrationStatus::PlanningPreset` |
| 28 | Metadata classification for Airglow component uses `SiteProfileId` mapping | (4) | `evaluator/metadata.rs::component_status_for_site_profile` |
| 29 | Fallback seasonal correction value `unwrap_or(1.0)` | (4) | `continuum.rs::evaluate_continuum_with_time_bin` |
| 30 | Baseline template provenance and checksum are attached at bundle/load time | (2) | `calibration.rs` checksum assertion + embedded `Provenance::bundled_file(...)` |
| 31 | Time/season/solar/Van Rhijn correction model domain is limited to astronomical night | (2) | `continuum.rs` returns zero if `time_of_night_bin(time, location)` is `None` |
| 33 | Baseline wavelength-resolved relative mean spectrum | (2) | `calibration.rs::load_builtin_standard` builds `AirglowContinuum.spectrum` from `airglow_cont.dat` `relative_mean` column |
| 34 | Baseline wavelength-resolved relative uncertainty spectrum | (2) | `calibration.rs::load_builtin_standard` builds `AirglowContinuum.uncertainty` from `airglow_cont.dat` `relative_sigma` column |
| 35 | Integrated relative 300–650 nm shape computed from baseline spectrum | (2) | `calibration.rs` computes `integrated_relative_300_650` via `algo::trapz_range(..., WL_LOW_NM, WL_HIGH_NM)` |
| 36 | Integrated absolute uncertainty shape computed from baseline uncertainty spectrum | (2) | `calibration.rs` computes `integrated_uncertainty_abs_300_650` via `algo::trapz_range` on `uncertainty_abs` |
| 37 | B/V relative diagnostics derived from baseline spectrum interpolation at 445/551 nm | (2) | `calibration.rs::load_builtin_standard` computes `b_relative` and `v_relative` via `algo::interp_linear(..., B_FILTER_NM/V_FILTER_NM)` |
| 38 | Relative uncertainty aggregation is computed as quadrature of level and shape | (4) | `continuum.rs::evaluate_continuum_with_time_bin` uses `level_relative_uncertainty.hypot(shape_relative_uncertainty)` |
| 39 | Time-of-night bin uses UTC→TT conversion before phase/search | (1) | `temporal.rs::utc_time_to_tt_mjd` uses `time.to::<TT>().to::<MJD>()` |
| 40 | Astronomical-night bracketing search parameters (adaptive window) | (4) | `temporal.rs` constants: `INITIAL_NIGHT_SEARCH_RADIUS`, `MAX_NIGHT_SEARCH_RADIUS`, `NIGHT_SEARCH_EXPANSION_FACTOR` |
| 41 | Upstream licence and exact upstream release are not recorded in repo | (5) | `crates/nsb/data/manifest.toml` fields `source`/`license` for `airglow_cont.dat` |

Notes:
- Item (5) is used for provenance recovery gaps where the repository cannot establish upstream dataset release identity or redistribution/licence terms.

## Baseline continuum audit: `crates/nsb/data/airglow_cont.dat`
### Provenance we can establish from repo evidence
1. **Bundled asset path and schema version**  
   **Origin**: `crates/nsb/data/manifest.toml` entry `path = "airglow_cont.dat"` and `schema = "skycalc-airglow-continuum-v1"`.

2. **Checksum is pinned**  
   **Origin**: `crates/nsb/src/components/airglow/calibration.rs` checksum assertion and `manifest.toml` sha256 value:
   `d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f`.

3. **SkyCalc/Noll-style empirical continuum table (source label)**  
   **Origin**: `manifest.toml` `source = "ESO SkyCalc/Noll-style empirical continuum table from a historical import; exact upstream release is not recorded"`.

4. **Original wavelength domain (from embedded file header comments)**  
   **Origin**: `airglow_cont.dat` header lines:
   - `lam rflux drflux (wavelengths [mum], relative fluxes, and uncertainties)`
   - template wavelengths include `0.3` through `2.5` (with non-zero relative fluxes only in the relevant region).

5. **Original wavelength scale unit**  
   **Origin**: `airglow_cont.dat` comment explicitly says `wavelengths [mum]`.

6. **Template normalization / adaptation**  
   **Origin**: file header comment:
   - `(adaption for 0.543 mum only -> constant shape)`
   - `Airglow continuum scaling data for C543` and `lam ... 0.543 1.0 ...`

7. **Typical emission height and scaling blocks**  
   **Origin**: `airglow_cont.dat`:
   - `height (typical altitude of emission [km]) = 90`
   - `scale (global scale factor for tabulated fluxes) = 79.829`
   - solar activity correction `cons slope = 0.2068... 0.06139...`

8. **Time/season/solar correction model description**  
   **Origin**: `airglow_cont.dat` header comments:
   - “Season: 6 double months starting with Dec+Jan”
   - “Time: 3 periods (equal lengths; full range: alt_sun < -18)”
   - “Full correction: relative mean * solar activity corr. * season/time corr.”
   - “Additional corrections: airmass and extinction”

9. **Units used by NSB when interpreting the `scale` block**  
   The loader treats the computed `scale` as SkyCalc spectral photon radiance via:
   - `components/airglow/continuum.rs` uses `SkyCalcSpectralPhotonRadiance::new(scale)`
   - `crates/nsb/src/units.rs` defines `SkyCalcSpectralPhotonRadiance` as:
     `photons s⁻¹ m⁻² arcsec⁻² µm⁻¹`.

### Provenance we cannot fully establish (explicit unresolved)
The repository does not contain enough evidence to answer:
1. **Whether the continuum is tied to Paranal vs another observatory**  
   There is no “Paranal” or site identifier in `airglow_cont.dat`, and `manifest.toml` does not record an observatory tie beyond a SkyCalc lineage label.
   **Classification**: (5) unresolved / insufficient provenance.

2. **Exact upstream SkyCalc release identity**  
   **Origin**: `manifest.toml` says “exact upstream release is not recorded”.  
   **Classification**: (5).

3. **Upstream redistribution and license terms**  
   **Origin**: `manifest.toml` `license = "upstream dataset license is not recorded; this blocks calibrated-production promotion"`.  
   **Classification**: (5).

## Hidden site dependence audit (Paranal/CTAO whitelist risk)
The Airglow default path uses three location-dependent computations:
1. `target_altitude(...)` uses the caller `Observer` location and UTC time.
2. `season_bin(...)` uses caller longitude to compute local solar date/month.
3. `time_of_night_bin(...)` uses caller location to compute astronomical night intervals via Siderust solar-altitude events.

The Airglow evaluation does **not** use:
- site pressure / Rayleigh scale height / aerosol Mie parameters
- site atmosphere state

Those are computed when building `SiteProfile` in `crates/nsb/src/site.rs`, but the Airglow evaluator multiplies only by `profile.airglow.scale` (currently neutral 1.0 for built-ins).

Therefore:
- **Location is treated as an input**, not a whitelist.
- There is no silent Paranal/CTAO inheritance in the Airglow continuum math.

Remaining uncertainty:
- the bundled baseline continuum itself could embed “Additional corrections: airmass and extinction” that may have been generated for a particular reference atmosphere; repo evidence does not record the upstream atmosphere parameters.
This is accounted for as a generic empirical baseline with unresolved “embedded atmospheric/site assumptions”.

## Audit outcome decision (A/B/C/D)
### Chosen outcome: A — current baseline is defensible generic
Evidence:
1. The implementation applies the bundled continuum as a global template shared by all built-in site profiles.
2. The only site-profile-dependent Airglow term in the default pipeline is the explicit multiplicative `profile.airglow.scale`.
3. All built-in CTAO profiles are explicitly declared as *planning presets* with neutral airglow scale (1.0), rather than calibrated continuum assets.
4. Repo evidence does not establish a Paranal-only tie for the bundled continuum.

Limitations (why this is still an audit, not full historical certainty):
1. Upstream exact release identity and redistribution/license status are unresolved in the repository.
2. Embedded upstream “airmass and extinction” details are not fully attributable to repo evidence.

## Relationship to other issues
- #109 (F10.7 resolver): Not modified by this Phase 1 audit boundary work.
- #110 (alternative geometry model): Not modified.
- #38 (CTAO scientific calibration): Not modified; CTAO scientific calibration remains handled there.

## Recommended remediation (minimal, Phase 1)
After this audit doc is created, Phase 1 will:
1. Strengthen the runtime/API/CLI metadata so the bundled baseline asset identity (path/schema/checksum) is explicit and cannot be mistaken for calibrated site data.
2. Add targeted regression tests enforcing:
   - arbitrary Earth location uses the generic path (no Paranal/CTAO required)
   - CTAO planning presets remain non-calibrated in metadata
   - baseline asset identity/checksum is deterministic
   - location-dependent quantities depend on caller location
   - named site choice does not alter baseline-only parameters when the calibration object is neutral.

