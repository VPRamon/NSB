# Model maturity

Status: Current maturity policy exposed through API and CLI metadata.
Audience: Scientific users, reviewers, and downstream consumers.
Scope: Component maturity labels, validated domains, and allowed production
claims.
Non-goals: This document does not provide the validation evidence itself; see
[Validation matrix](validation.md).

Software release readiness, geographic support, data provenance, and scientific
site calibration are separate axes.

For Airglow, the current “Generic/planning” runtime label does not establish a
scientifically generic data product. #200 rejects retaining the Paranal-trained
baseline as a reference/fallback solution. The
[Phase 1 survey](../nsb_components/airglow/validation/generic-model-decision-200.md)
admitted no replacement; the table below describes existing software only.

| Surface | Status | Validated domain | Production claim allowed |
|---|---|---|---|
| Evaluator composition and units | Production software | Typed deterministic composition and component-sum identity | Yes, for software behaviour |
| Atmospheric transport (`nsb::transport`) | Foundation / planning software | Identity exactness; Beer–Lambert direct path with Siderust Rayleigh/Mie/(optional) ozone; origin-rejection for airglow/moonlight | Planning / software only; does not imply site calibration |
| Zodiacal component | Generic clear sky | Leinert anchors and Noll-style formula checks | Planning only |
| Airglow component | Generic/planning | Astronomical night with a Paranal-derived continuum; arbitrary-location geometry is supported, but no location (including Paranal) is automatically site-calibrated | Planning only |
| Jones 2013 moonlight | Generic/planning | Spectral computation and deterministic regression cases | Planning only |
| KS91 moonlight | Published reference | Approximate Johnson-V analytic benchmark | Reference-model use |
| CTAO-N and CTAO-S | Planning preset | Explicit pressure/aerosol/airglow assumptions selected by application-layer `SiteProfileTag` markers (for example `nsb-cli`), independently of observatory identity | No calibrated-site claim |
| Combined 300–650 nm bundled starlight candidate | Frozen release candidate; redistribution pending (#103) | Exact nside128 candidate-v5 checksum-pinned; covariance-corrected bright-star supplement included; technical and external cross-implementation validation pinned in the release-candidate bundle | Not yet; requires the authorized redistribution decision from #103 and a follow-up production-activation change. #103 does not block the 0.1.0 MVP while the asset remains non-embedded |
| Validated external starlight | Production for the sidecar-declared domain | Runtime integrity, schema, HEALPix, flux evidence, contrast/seam, calibrated photometry, and independent-comparison contract | Yes, only as justified by the reviewed external evidence |
| Caller experimental starlight map | Experimental | Map schema/value checks | No production claim |
| B/V S10 and magnitudes | Proxy diagnostic | 445/551 nm central-wavelength convention | No passband-photometry claim |

For Airglow, the bundled continuum's Paranal/Noll/SkyCalc/FORS1 lineage records
source provenance, not calibration evidence. Observer coordinates, observatory
identity, geometry, F10.7, atmospheric/extinction assumptions, and user scaling
may change the numerical result or provenance but cannot promote scientific
maturity.

Public typed profile construction is fail-closed with respect to maturity:
`SiteProfile::<GenericClearSky>::generic_clear_sky()` produces
`GenericFallback`, `SiteProfile::<P>::planning(...)` produces
`PlanningPreset` for any external marker `P`, and the public API does not expose
a path to construct `CalibrationStatus::Calibrated` through caller-defined tags.
A profile therefore cannot promote its own component metadata to `Production`
merely by supplying atmospheric parameters or a custom `NAME` string. Airglow
evaluation always uses the bundled continuum template regardless of marker type.

A future `Calibrated` runtime profile must be connected to dedicated admitted
site-validation evidence. Issue #38 remains the scientific promotion gate.

`ValidatedExternalMap` is explicit and remains outside `ComponentMask::ALL`.
Failure of any admission check is an error; it never falls back to an
experimental seed map.
