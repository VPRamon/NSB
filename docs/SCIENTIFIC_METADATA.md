# Scientific metadata and uncertainty contract

`nsb` returns scalar radiance estimates for planning and analysis, but the value
alone is not sufficient scientific context. Every public `NsbComponent` therefore
carries `NsbComponentMetadata` alongside its integrated 300–650 nm radiance and
B/V diagnostics.

## Calibration status

`CalibrationStatus` is a compact maturity label for downstream planning systems:

| Status | Meaning |
|---|---|
| `Production` | Deterministic model or dataset validated for the current release. |
| `GenericClearSky` | Generic clear-sky or planning assumption; useful but not site-calibrated. |
| `Proxy` | Diagnostic approximation that must not be interpreted as calibrated science output. |
| `Legacy` | Historical model retained for regression or compatibility. |
| `Experimental` | Implemented capability without a production validation contract yet. |

The current default `NsbModelConfig::generic_clear_sky()` intentionally reports
`GenericClearSky` for the active zodiacal, airglow, and Jones 2013 moonlight
components. This prevents callers from mistaking the default planning baseline
for a CTAO-validated production science preset.

## B/V diagnostic convention

The public `b_flux_s10`, `v_flux_s10`, `b_mag`, and `v_mag` outputs use
`BandDiagnostic::MONOCHROMATIC_S10_PROXY`:

- B diagnostic wavelength: 445 nm.
- V diagnostic wavelength: 551 nm.
- Surface-brightness zero point: `NSB_S10_ZP = 27.78`.
- Convention string: `monochromatic-central-wavelength-s10-proxy`.

These fields are not Johnson B/V passband integrations. They are central-
wavelength S10 diagnostics retained for engineering inspection, legacy
comparisons, and quick planning output. Publication-quality band photometry
should use documented passband response curves and an explicit spectral
integration path.

The integrated radiance field remains independent from these diagnostics and is
reported over the 300–650 nm photon-radiance band.

## Component provenance and first-order uncertainty budget

### Zodiacal light

Provenance: Leinert et al. (1998) S10 zodiacal table, Noll et al. (2012)
approximate atmospheric extinction, and the bundled solar spectrum.

Status: `GenericClearSky`.

Current quantitative gates:

- exact table-anchor checks for selected Leinert grid values;
- bit-for-bit parity against the legacy bilinear lookup path;
- numeric reference check for the Noll-style extinction transmission;
- separate exoatmospheric and observed-path tests.

Primary uncertainty terms:

- interpolation and extrapolation near missing Leinert table corners;
- generic, non-site-calibrated atmospheric extinction;
- dependence on the solar spectrum and reddening approximation;
- geometry sensitivity near the Sun and near table boundaries.

No component-level relative uncertainty is currently exposed for zodiacal light.
Callers should treat the metadata status and provenance as the validity signal.

### Airglow continuum

Provenance: bundled SkyCalc-derived empirical continuum calibration in
`NSB/data/airglow_cont.dat`, including mean seasonal/time-of-night correction
matrices, sigma correction matrices, a relative mean spectrum, and a relative
uncertainty spectrum.

Status: `GenericClearSky`.

NSB exposes `NsbComponent::relative_uncertainty` for airglow when the query
produces non-zero physical emission. The reported value is the relative
integrated one-sigma estimate derived from the same time/season correction bin
and wavelength-dependent uncertainty spectrum as the emitted continuum.

Primary uncertainty terms:

- intrinsic airglow variability on minute-to-season timescales;
- site mismatch when the bundled empirical template is used away from its
  calibration assumptions;
- solar-radio-flux correction uncertainty;
- Van Rhijn geometry approximation and finite emission-height assumption.

### Scattered moonlight

Provenance: Jones et al. (2013) wavelength-resolved lunar reflectance/scattering
model, Siderust lunar geometry, bundled solar spectrum, and a generic clear-sky
atmosphere. The legacy alternative is Krisciunas & Schaefer (1991).

Status: `GenericClearSky` for Jones 2013 in the default evaluator;
`Legacy` for Krisciunas & Schaefer.

Current quantitative gates:

- explicit atmospheric-condition constructors for generic clear sky,
  Paranal-like, CTA-S planning, and CTA-N planning assumptions;
- a structured reference-case fixture covering lunar phase angle, Moon-target
  separation, Moon zenith distance, source zenith distance, wavelength samples,
  integrated 300–650 nm radiance, and tolerances;
- tests documenting the empirical Jones Mie weighting factor as a calibration
  knob rather than a physical constant.

Primary uncertainty terms:

- aerosol optical depth and Mie phase-function mismatch;
- single-scattering simplification and empirical Mie weighting;
- non-site-calibrated default atmospheric profile;
- lunar albedo/spectrum approximation and geometry sensitivity for small
  Moon-target separations.

No component-level relative uncertainty is currently exposed for moonlight; the
metadata status must be propagated to planning consumers.

### Integrated starlight

Provenance: disabled by default. Custom maps report caller-supplied map
provenance. The future bundled catalogue map remains experimental until it is
generated, bundled, and quantitatively validated.

Status: `Experimental` when a starlight map is explicitly configured.

Primary uncertainty terms:

- catalogue completeness and masking;
- Galactic-coordinate map resolution;
- spectral assumptions used to convert catalogue fluxes to the NSB band;
- map provenance and validation quality.

`ComponentMask::ALL` intentionally excludes starlight until the bundled catalogue
map has a production validation contract. Use `ComponentMask::ALL_SUPPORTED` only
with an explicit `StarlightModel`.

## Downstream use

Planning systems should propagate at least:

1. integrated radiance;
2. component names and component radiances;
3. `CalibrationStatus`;
4. provenance strings;
5. `relative_uncertainty` where available;
6. the `BandDiagnostic` convention if B/V fields or magnitudes are displayed.

Consumers must not collapse `GenericClearSky`, `Legacy`, or `Experimental` into a
single science-quality label. These statuses are deliberately separate because
they represent different validation risks.
