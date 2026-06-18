# End-to-end NSB validation

This document records the validation contract for the public `nsb` evaluator.  The
component tests protect the individual model implementations; the end-to-end
suite protects the summed production prediction and the threshold-window planner
surface.

## Validation tiers

### CI deterministic validation

`crates/nsb/tests/end_to_end_validation.rs` is the lightweight validation suite
that must run in ordinary CI.  It uses only bundled data and deterministic
astronomy inputs, so it is suitable for pull requests and release gating.

The CI suite covers:

| Gate | Purpose | Failure indicates |
|---|---|---|
| Production `ComponentMask::ALL` point envelopes | Evaluate dark-time, moonlit, near-Galactic-plane, high-Galactic-latitude, and twilight-boundary planning cases at CTAO-S. | Non-physical total NSB, unit-scale regression, or accidental production preset change. |
| Component-sum conservation | Check that the reported total integrated radiance equals the sum of reported component radiances. | Composition bug, duplicated component, dropped component, or unit-conversion bug. |
| Explicit starlight fixture contrast | Use the synthetic test starlight map with `ComponentMask::ALL_SUPPORTED` and verify Galactic-plane output remains brighter than high-latitude output. | Broken target-to-Galactic-map plumbing or starlight composition. |
| Sampled threshold reference curve | Build an independent point-sampled curve and require `periods_below_threshold` to classify sampled points consistently. | Threshold crossing, comparison, or interval-complement regression. |
| Independent observability intervals | With an unrestrictively high threshold, require returned windows to equal the independent intersection of Sun-below and target-above intervals. | Search pre-filter or event-boundary regression. |

The production point envelopes are deliberately broad.  They are not intended to
replace observational calibration; they are CI guards against order-of-magnitude
mistakes and non-physical values.  Tight numerical regression is enforced where
it is implementation-stable: reported total equals component sum, threshold
classification agrees with a sampled public point-evaluation curve, and
unrestrictive thresholds reduce to independently computed observability windows.

### External reference validation

Large external validation datasets should live outside the ordinary crate build
or under a future opt-in dataset target.  Recommended sources are:

- ESO SkyCalc outputs for representative moon phase, target, airmass, and
  twilight configurations.
- Published dark-sky and moonlit-sky benchmark cases for Paranal-like sites.
- Site measurements or engineering reference curves from CTAO commissioning or
  operations when those data are cleared for repository use.

External fixtures should be stored as tabular data with explicit provenance:

```text
case_id,site,longitude_deg,latitude_deg,height_m,time_utc,ra_deg,dec_deg,
components,reference_source,reference_band,reference_value,reference_unit,
accepted_abs_tolerance,accepted_rel_tolerance,notes
```

A comparison test should convert every reference value to
`PhotonsPerSquareCentimeterNanosecondSteradian` before comparison.  Any accepted
deviation must be classified as either:

- **model choice**: e.g. different lunar scattering model, atmosphere profile,
  aerosol assumption, passband, or solar activity assumption;
- **data limitation**: e.g. unavailable production starlight map or measurement
  quality flag;
- **implementation error**: a defect that should block release.

## Tolerance policy

Use tight tolerances only for deterministic implementation identities, such as
component-sum conservation and interval boundary agreement with the same time
scale.  Use physically justified tolerances for external references, and record
those tolerances with the fixture rather than burying them in test code.

Current CI tolerances:

| Quantity | Tolerance |
|---|---:|
| Component-sum conservation | `1e-12 * max(total, 1)` |
| Event-boundary agreement | 120 seconds |
| Production reference envelope | case-specific broad physical envelope |

The 120-second event-boundary tolerance is intentionally looser than the current
implementation usually needs.  It protects the CI gate from harmless upstream
changes in iterative event refinement while still catching planner-scale
regressions.

## How to run

```bash
cargo test -p nsb --test end_to_end_validation
cargo test --workspace
```

The workspace CI workflow runs these tests automatically for pull requests and
pushes to `main`.
