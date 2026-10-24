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
| Airglow default | checksum-pinned PALACE v1 continuum product; official PALACE continuum cross-check; month/time/F10.7/runtime regressions; diagnostic reference comparison | R/nm source; ph cm⁻² ns⁻¹ sr⁻¹ runtime, 300–650 nm plus 445/551 nm diagnostics | byte-identical regeneration; PALACE integral relative difference `3.88e-7`; strict stored output tolerances | Implementation error, model-semantic difference, or documented planning limitation |
| Airglow vertical geometry | synthetic thin/broad/two-layer profiles; spherical LOS refinement; observer-height cases | dimensionless geometry factor | zenith exact; thin-shell relative tolerance derived from 20 m profile; refinement convergence | Implementation error; profile representativeness remains a data limitation |
| Experimental starlight | synthetic contrast and HEALPix completeness | proxy radiance plus S10 diagnostics | deterministic | Implementation error only; no science claim |
| Validated external starlight admission | caller map plus TOML sidecar | declared calibrated integrated band plus B/V diagnostics | exact integrity/header checks; plane/pole >= 1; seam jump <= 1; declared flux tolerance | Implementation error or rejected caller evidence |
| Jones spectral fixture | Jones-model regression rows | 300–650 nm | 20% fixture tolerance | Data limitation/regression |
| Solar spectrum | pinned NSB Planck-model specification using IAU nominal solar values and exact SI constants | W m^-2 nm^-1, 300–650 nm | exact regeneration; input/output checksums and redistribution terms; 351-point grid and exact anchors; integral/B-V regression; Zodiacal/Jones impact bounds | Implementation, provenance, licensing, or scientific-impact regression |
| CTAO-N/S | explicit assumptions only | atmosphere and airglow profile | none | Data limitation |

## Public observational notebook

The [NSB versus nsb2 notebook](../../examples/validation/NSB_Validation.ipynb)
implements the registered Patat 365–650 nm comparison with explicit continuum
and experimental Oxygen selection. Its [protocol and setup](../../examples/validation/README.md)
include verified public data, source/wheel identity, matched measured F10.7, paired
night-cluster statistics, component diagnostics and checksummed result records.
All execution logic lives in the single notebook; no companion package is required. Ordinary
CI executes synthetic fixtures only; the real campaign is manually triggered.
Patat remains previously examined development evidence, not an independent Oxygen
holdout or a change to scientific admission/defaults.

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

These diagnostics validate construction, not astrophysical calibration. The registered compact production map is distinct from these experimental
products; construction checks alone do not demonstrate observational accuracy.

## Validated external starlight

Caller-supplied production overrides undergo admission that verifies SHA-256,
manifest schema/completeness, exact header values, full Galactic HEALPix
coverage, finite/nonnegative values, plane/pole contrast, and longitude-wrap
continuity. If input B/V flux totals and tolerance are present together, flux
conservation is recomputed. Otherwise the sidecar must still attest that flux
conservation was validated and point to its report. Production admission also
requires a non-proxy photometry identifier and an independent comparison.

These checks establish a fail-closed evidence contract; they do not make an
unreviewed caller claim true. Scientific users remain responsible for reviewing
the referenced catalogue license, calibration, and comparison report.

## Bundled Starlight admission

The registered compact runtime map and sidecar are active in four-component
defaults, with checksum-specific conditional redistribution approval under #103.
The current decision, permitted channels and immutable previous approvals live in
[the release-candidate bundle](../nsb_components/starlight/release-candidate/README.md).
The bundle README and attribution are frozen approval inputs and retain historical
pending-approval wording; the current decision controls distribution status.
Repository-only candidate CSVs remain excluded from runtime packages. Integrity,
redistribution approval and cross-implementation agreement do not establish
independent observational accuracy.

## Independent observational evidence

The authoritative reports live in
[nsb-validation](https://github.com/VPRamon/nsb-validation). Internal numerical
regressions, synthetic solver checks and physical assumptions are separate from
observational admission. No new campaign certifies this release candidate.

- [Patat/FORS1 benchmark](https://github.com/VPRamon/nsb-validation/blob/main/reports/patat-fors1-spectral-v1/report.md):
  103 conditionally admitted spectra on 22 nights; held-out evaluation uses 94
  spectra on 17 nights over 365–650 nm. The evaluated NSB revision is
  `1b2556727205694c7765ef0d254c5d1b301582ec`, not the upcoming release.
  Relative MAE is approximately 0.361 for NSB, 0.171 for nsb2 and 0.152 for the
  calibration-night baseline. Missing airglow lines drive substantial
  underprediction; Starlight redistribution and component compatibility require
  the report's explicit approximations.
- [Oxygen development diagnostic](https://github.com/VPRamon/nsb-validation/blob/main/reports/airglow-diagnostic-v1.md):
  newer oxygen-model scores reuse the inspected cohort and are development
  evidence, not independent generalization or validation of the next release.
- [DESI pilot](https://github.com/VPRamon/nsb-validation/blob/main/reports/desi-dr1-pilot-v1/report.md):
  three independent nights, not 21 independent observations from seven bands.
  Instrument response, diffuse calibration, spectral completeness and observable
  equivalence prevent admitted absolute predictive accuracy.
- [LVM admission](https://github.com/VPRamon/nsb-validation/blob/main/reports/lvm-absolute-admission-v1/report.md):
  reconstruction/acquisition has progressed, but diffuse calibration,
  uncertainty and complete observable equivalence still block absolute accuracy.

No independent campaign establishes CTAO site calibration or complete optical
predictive accuracy. See the [consumer contract](validation-consumer.md) for
identifying a new campaign without relabelling historical results.

## Commands

```bash
cargo test -p nsb --test end_to_end_validation --locked
cargo test -p nsb --test jones2013_validation --locked
cargo test -p nsb --test science_metadata --locked
cargo test --workspace --locked
```
