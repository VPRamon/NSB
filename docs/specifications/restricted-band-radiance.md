# Restricted component-band radiance

`NsbEvaluator::integrate_band(query, lower_nm, upper_nm)` integrates a top-hat
sub-band wholly within 300–650 nm, returning photons cm^-2 ns^-1 sr^-1. Rust
bounds and results are typed quantities; Python accepts bounds in nm and returns
a scalar in those documented units. Existing point/planning APIs are unchanged.

This method integrates the existing Zodiacal and PALACE continuum samples using
the same geometry, solar activity, climatology, atmospheric propagation, scale
and piecewise-linear interpolation as the original evaluator. It introduces no
new physical asset, emission line, fitted factor or calibrated photometric system.
The full band reproduces the current integrated evaluation for supported components.
Sub-band boundaries interpolate the already sampled radiance, never extrapolate.

Starlight requests fail: the map does not preserve a wavelength-resolved product.
Moon requests succeed only when the existing moonlight evaluator returns physical
zero; nonzero lunar sub-bands fail explicitly. This allows default moonless queries
where the build has no bundled starlight, without pretending to implement a general
spectral lunar API. Bounds, geometry and component support are validated before use.

Missing emission lines, omitted starlight and the Paranal-derived continuum's
transfer to other sites remain limitations in prediction accuracy. They do not
change the meaning or units of a valid restricted-band prediction. Observations
must be independently calibrated to the same ground-level photon radiance and
wavelength interval before residuals can be interpreted. No Johnson/FORS/AB/Vega
photometric equivalence is implied by a top-hat band.

## Component-separated restricted observables

`NsbEvaluator::integrate_band_components(query, lower, upper)` returns a typed
`RestrictedBandResult`: bounds in nanometres, summed photon radiance and named
`RestrictedBandComponent` contributions with descriptor provenance and explicit
unknown statistical/systematic band uncertainties. Python exposes the same
records with `lower_nm`, `upper_nm`, `integrated_photons_cm2_ns_sr`, and
`components`. This avoids reusing the 300–650 nm `NsbResult` or its B/V proxies
for a different physical observable. It supports ablations without fitting any
component to observations. All requested components must pass existing gates;
unsupported Starlight/nonzero Moon never produces a partial successful result.

Propagation follows existing component configuration; selecting
`ZodiacalExtinction::None` still selects TOA zodiacal light. The API performs no
additional atmospheric propagation or instrument convolution. These integrals
must not be treated as new unattenuated sources. Descriptor metadata retains
model/site maturity, and Airglow adds the resolved F10.7 provenance; full-band
physical evaluation/uncertainty records are not relabelled as sub-band records.
No Airglow lines, DGL, calibrated response, or new model maturity are implied.
