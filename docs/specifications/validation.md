# Validation matrix

Status: Current repository validation map.
Audience: Scientific reviewers, maintainers, and downstream users evaluating
fitness for use.
Scope: Evidence, units, tolerances, deviation classes, and known missing
external campaigns.
Non-goals: This document does not certify any future bundled starlight asset or
site-calibrated CTAO product.

NSB distinguishes implementation identities, published references, external
observations, and sanity envelopes. A broad envelope is not external validation.

| Area | Evidence | Units/band | Tolerance | Deviation class |
|---|---|---|---|---|
| Component sum | `end_to_end_validation.rs` | ph cm⁻² ns⁻¹ sr⁻¹, 300–650 nm | `1e-12 * max(total,1)` | Implementation error |
| Window classification | sampled public evaluator curve | typed radiance and UTC | classification identity | Implementation error |
| Event boundaries | independent Siderust interval intersection | UTC seconds | 120 s | Implementation error/upstream refinement |
| Zodiacal table | Leinert selected anchors | S10 diagnostic | exact anchors | Implementation error |
| Noll extinction | numeric formula fixture | dimensionless transmission | `1e-12` absolute | Implementation error |
| KS91 full Moon | `external_reference_cases.csv` citing PASP 103, 1033 | approximate Johnson V mag arcsec⁻² | 0.7 mag | Model choice |
| Airglow | deterministic temporal/domain checks | 300–650 nm plus 445/551 nm diagnostics | implementation-specific | Implementation error |
| Experimental starlight | synthetic contrast and HEALPix completeness | proxy radiance plus S10 diagnostics | deterministic | Implementation error only; no science claim |
| Validated external starlight admission | caller map plus TOML sidecar | declared calibrated integrated band plus B/V diagnostics | exact integrity/header checks; plane/pole >= 1; seam jump <= 1; declared flux tolerance | Implementation error or rejected caller evidence |
| Jones spectral fixture | inherited darknsb regression rows | 300–650 nm | 20% fixture tolerance | Data limitation/regression |
| CTAO-N/S | explicit assumptions only | atmosphere and airglow profile | none | Data limitation |

## Threshold Window Contract

Threshold-window search uses prepared physical intervals to reduce repeated
event work:

- Sun-altitude and target-visible periods define candidate windows;
- astronomical-night periods define airglow time-of-night bins during threshold
  sampling;
- Moon-up periods skip moonlight evaluation only when the Moon is below the
  horizon by the same Siderust event machinery used elsewhere.

These intervals are search acceleration, not replacement science. When a sample
or crossing is evaluated inside a relevant interval, NSB calls the exact
component evaluator. Candidate windows are further split at airglow phase and
Moon-visibility boundaries before adaptive search, so no smoothness assumption
crosses a physical regime boundary. Adaptive acceptance is limited to exact
samples that are clear of the threshold; short or unclear intervals use the
bounded scan fallback. Boundary refinements remain bounded by the documented
event/crossing tolerance; changes larger than that are treated as implementation
defects unless a reviewed upstream event-refinement change explains them.

## Independent reference fixture

`crates/nsb/tests/data/external_reference_cases.csv` stores source, locator,
quantity, unit, band, expected value, tolerance, assumptions, and deviation
class. The current cleared case is the published Krisciunas & Schaefer (1991)
full-Moon reference. It validates the alternate analytic model, not the Jones
spectral default or CTAO site calibration.

Required classifications are:

- `implementation-error`: code or unit defect; blocks release;
- `model-choice`: difference caused by a documented physical model or band;
- `data-limitation`: missing provenance, catalogue depth, site data, or quality.

## Bundled asset validation

Run:

```bash
nsb-data dataset <dataset> validate --config <run.toml>
```

The verifier rejects an unsupported manifest schema, empty required metadata,
duplicate paths, missing/unregistered files, invalid or drifting SHA-256 values,
and configured asset-header mismatches. Runtime starlight loading repeats the
manifest/header contract check and uses a compile-time checksum assertion.

Unknown source releases or licenses are recorded explicitly in the manifest.
They are promotion blockers, not verifier exceptions.

## Experimental starlight

The generation pipeline validates complete HEALPix coverage, finite/nonnegative
values, flux conservation, Galactic plane/pole contrast, and longitude wrapping.
It can emit a JSON diagnostics report containing source/pixel counts, totals,
diagnostic pass/fail values, photometry model, maturity, and output checksum.

These diagnostics validate construction, not astrophysical calibration. A real
catalogue release with reviewed redistribution terms plus independent sky
brightness comparison is required before starlight enters defaults.

## Validated external starlight

The production API accepts no bundled substitute. Admission verifies SHA-256,
manifest schema/completeness, exact header values, full Galactic HEALPix
coverage, finite/nonnegative values, plane/pole contrast, and longitude-wrap
continuity. If input B/V flux totals and tolerance are present together, flux
conservation is recomputed. Otherwise the sidecar must still attest that flux
conservation was validated and point to its report. Production admission also
requires a non-proxy photometry identifier and an independent comparison.

These checks establish a fail-closed evidence contract; they do not make an
unreviewed caller claim true. Scientific users remain responsible for reviewing
the referenced catalogue license, calibration, and comparison report.

## Gaia DR3 bundled starlight candidate

The Gaia DR3 release pipeline now has maintainer tooling for extraction
documentation, passband source preparation, HEALPix map generation, validation
reporting, and asset packing. CI uses tiny fixtures only. A real bundled asset
is not production until the Gaia extract checksum, canonical input checksum, map
checksum, validation report, longitude seam diagnostic, and structured
independent regional comparison are all reviewed. The independent reference file
declares regions and expected radiance ranges; it does not provide trusted pass
booleans.

## Missing external campaigns

No cleared fixtures currently validate dark-sky totals, Jones spectral moonlight,
twilight, low/high Galactic latitude, or CTAO-N/S site profiles against SkyCalc
or observatory measurements. Those surfaces remain generic/planning/experimental
in metadata. They must not be described as calibrated.

## Commands

```bash
cargo test -p nsb --test end_to_end_validation --locked
cargo test -p nsb --test jones2013_validation --locked
cargo test -p nsb --test science_metadata --locked
cargo test --workspace --locked
```
