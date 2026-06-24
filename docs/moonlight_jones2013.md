# Jones 2013 spectral moonlight validation

`Jones2013Spectral` implements wavelength-resolved scattered moonlight using the Jones et al. 2013 lunar reflectance/scattering formulation as provided through Siderust lunar photometry plus NSB's bundled solar spectrum, Mie phase grid, and multiple-scattering correction grid.

## Validated domain

The validation target is the optical planning band used by NSB:

- wavelength range: 300-650 nm;
- Moon above horizon;
- target above horizon;
- positive Moon-target separation;
- topocentric Moon distance greater than zero;
- clear-sky atmospheric conditions selected explicitly by the caller.

Outside that domain the implementation returns zero for non-observable geometry or propagates component errors through the evaluator.

## Atmospheric conditions

`AtmosphericConditions` deliberately contains only atmospheric properties:

- surface pressure;
- Rayleigh scale height;
- Mie/aerosol optical-depth parameters.

It does not contain observer altitude. Altitude is taken from the `Geodetic<ECEF>` location supplied to `Jones2013Spectral`, so callers cannot accidentally combine a site profile from one observatory with the altitude of another site.

The available constructors are:

- `AtmosphericConditions::generic_clear_sky(location)`: altitude-derived fallback; not site calibrated;
- `AtmosphericConditions::paranal_average()`: Siderust's built-in Paranal-like average atmosphere;
- `AtmosphericConditions::cta_s_clear_sky()`: explicit CTA-S planning preset; currently aliases the Paranal-like profile until dedicated CTA-S aerosol calibration data are bundled;
- `AtmosphericConditions::cta_n_clear_sky()`: explicit CTA-N planning preset using a La Palma/ORM-range pressure with the currently bundled clear-sky Mie parameterization.

The distinction between generic and site/planning presets is intentional. Production CTA science should use explicit presets or externally calibrated atmospheric profiles rather than relying on `standard_clear_sky`.

## Empirical Mie weight

`JONES_MIE_WEIGHT = 0.05` multiplies the aerosol/Mie phase term in the current implementation. It is an empirical calibration factor, not a physical constant. It compensates for the simplified single-scattering path and the bundled Mie grid used by NSB. Any change to that factor must be backed by quantitative reference spectra and should update the validation fixtures.

## Reference fixtures

`crates/nsb/tests/data/jones2013_reference_cases.csv` records the schema expected for external quantitative references:

```text
case_id,reference,phase_angle_deg,moon_target_sep_deg,moon_zenith_deg,source_zenith_deg,moon_distance_km,atmosphere,wavelength_nm,expected_ph_cm2_ns_sr_nm,expected_integrated_300_650,relative_tolerance
```

The initial fixture is seeded from the historical darknsb moonlight LUT. It is useful as a deterministic regression manifest for integrated scattered moonlight values, but it is not an independent SkyCalc or Jones-table validation campaign. Calibration requires independently generated spectral densities for matching geometry and atmospheric profiles.

## Expected accuracy

The current fixture tolerances are capped at 20% to reflect the accuracy expected from the literature-level sky model comparison and the current lack of site-specific aerosol phase-function calibration. Tighter tolerances are appropriate only when the fixture source, units, solar spectrum, atmospheric profile, aerosol model, and integration convention are all pinned.
