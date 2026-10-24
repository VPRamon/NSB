# Model maturity

Status: Current maturity policy exposed through API and CLI metadata.
Audience: Scientific users, reviewers, and downstream consumers.
Scope: Component maturity labels, validated domains, and allowed production
claims.
Non-goals: This document does not provide the validation evidence itself; see
[Validation matrix](validation.md).

Software release readiness, geographic support, data provenance, and scientific
site calibration are separate axes.

| Surface | Status | Validated domain | Production claim allowed |
|---|---|---|---|
| Evaluator composition and units | Production software | Typed deterministic composition and component-sum identity | Yes, for software behaviour |
| Atmospheric transport (`nsb::transport`) | Foundation / planning software | Identity exactness; Beer–Lambert direct path with Siderust Rayleigh/Mie/(optional) ozone; origin-rejection for airglow/moonlight | Planning / software only; does not imply site calibration |
| Zodiacal component | Generic clear sky | Leinert anchors and Noll-style formula checks | Planning only |
| Airglow Automatic (v0.2.0 PALACE Oxygen) | Experimental | Paranal-derived continuum plus Og/Or oxygen lines, 300–650 nm, standard continuum geometry and zenith distance ≤60°; no independent absolute admission | Planning / experimental only |
| Airglow explicit continuum (`ParanalPalaceV1`) | Generic/planning | Paranal-derived unresolved continuum with broader geometric coverage; custom validated vertical profiles require explicit continuum selection | Planning only |
| Jones 2013 moonlight | Generic/planning | Spectral computation and deterministic regression cases | Planning only |
| KS91 moonlight | Published reference | Approximate Johnson-V analytic benchmark | Reference-model use |
| CTAO-N and CTAO-S | Planning preset | Explicit pressure/aerosol/airglow assumptions selected by application-layer `SiteProfileTag` markers (for example `nsb-cli`), independently of observatory identity | No calibrated-site claim |
| Bundled Starlight | Approved compact production product | Exact 300–650 nm map and sidecar admitted through the production registry under #103 conditional approval | Only the sidecar/evidence-supported domain; registration is not new calibration |
| Validated external starlight | Production for the sidecar-declared domain | Runtime integrity, schema, HEALPix, flux evidence, contrast/seam, calibrated photometry, and independent-comparison contract | Yes, only as justified by the reviewed external evidence |
| Caller experimental starlight map | Experimental | Map schema/value checks | No production claim |
| B/V S10 and magnitudes | Proxy diagnostic | 445/551 nm central-wavelength convention | No passband-photometry claim |

For Airglow, the bundled PALACE v1.0 continuum's Paranal lineage records source
provenance, not calibration evidence. Observer coordinates, observatory
identity, geometry, F10.7, atmospheric/extinction assumptions, and user scaling
may change the numerical result or provenance but cannot promote scientific
maturity. Automatic Oxygen retains `ComponentCalibrationStatus::Experimental`
even under generic or CTAO planning site profiles. The profile's own
`CalibrationStatus` remains independent of component scientific maturity.

Public typed profile construction is fail-closed with respect to maturity:
`SiteProfile::<GenericClearSky>::generic_clear_sky()` produces
`GenericFallback`, `SiteProfile::<P>::planning(...)` produces
`PlanningPreset` for any external marker `P`, and the public API does not expose
a path to construct `CalibrationStatus::Calibrated` through caller-defined tags.
A profile therefore cannot promote its own component metadata to `Production`
merely by supplying atmospheric parameters or a custom `NAME` string. Airglow
evaluation uses the bundled continuum baseline regardless of marker type;
Automatic additionally includes the experimental PALACE Oxygen lines.

A future `Calibrated` runtime profile must be connected to dedicated admitted
site-validation evidence. Issue #38 remains the scientific promotion gate.

`ValidatedExternalMap` is explicit and remains outside `ComponentMask::ALL`.
Failure of any admission check is an error; it never falls back to an
experimental seed map.

## Registered Airglow fits and oxygen extension

`ParanalPalaceOxygenV1` is the **Automatic v0.2.0 planning fallback**, as well
as a supported explicit selection. It reconstructs the experimental Og/Or
oxygen subset on top of PALACE continuum; oxygen geometry and variability are
separate, with omitted species, molecular absorption and solar extrapolation
limitations stated in result metadata. This release-policy default is not an
independent absolute observational admission (Patat informed development; LVM
absolute equivalence remains pending). Automatic and explicit Oxygen both require
standard continuum geometry and zenith distance ≤60°; invalid geometry is
rejected during construction and out-of-domain queries error, even outside
astronomical night. Choose `ParanalPalaceV1` explicitly for continuum-only
historical comparisons, broader zenith coverage or custom vertical geometry.

Portable user continuum fits are reproducible **registered fits**, with recorded
training observations, covariance and provenance. Their component status is
`Experimental`; site status stays `PlanningPreset` and trusted calibrated status
remains false. User-configured planning assumptions, registered empirical fits,
and independently admitted scientific products are distinct. Neither an artifact
field, name, checksum nor successful synthetic recovery grants independent
validation. See the [workflow](../user-guide/airglow-calibration.md) and
[development evidence](../nsb_components/airglow/validation/oxygen-and-calibration-v1.md).
