# Model maturity

Status: Current maturity policy exposed through API and CLI metadata.
Audience: Scientific users, reviewers, and downstream consumers.
Scope: Component maturity labels, validated domains, and allowed production
claims.
Non-goals: This document does not provide the validation evidence itself; see
[Validation matrix](validation.md).

Software release readiness and scientific calibration are separate axes.

| Surface | Status | Validated domain | Production claim allowed |
|---|---|---|---|
| Evaluator composition and units | Production software | Typed deterministic composition and component-sum identity | Yes, for software behaviour |
| Zodiacal component | Generic clear sky | Leinert anchors and Noll-style formula checks | Planning only |
| Airglow component | Generic/planning | Astronomical night, continuum template domain | Planning only |
| Jones 2013 moonlight | Generic/planning | Spectral computation and deterministic regression cases | Planning only |
| KS91 moonlight | Published reference | Approximate Johnson-V analytic benchmark | Reference-model use |
| CTAO-N and CTAO-S | Planning preset | Explicit pressure/aerosol/airglow assumptions | No calibrated-site claim |
| Bundled starlight seed | Experimental proxy | Loader, HEALPix completeness, directional plumbing | No scientific claim |
| Gaia DR3 bundled starlight candidate | Pending production validation | Release-time Gaia XP/passband pipeline harness; Phase 5 XP continuous policy validated on independent holdout | Not yet; requires bulk XP continuous processing, integrated candidate, and independent sky validation |
| Validated external starlight | Production for the sidecar-declared domain | Runtime integrity, schema, HEALPix, flux evidence, contrast/seam, calibrated photometry, and independent-comparison contract | Yes, only as justified by the reviewed external evidence |
| Caller experimental starlight map | Experimental | Map schema/value checks | No production claim |
| B/V S10 and magnitudes | Proxy diagnostic | 445/551 nm central-wavelength convention | No passband-photometry claim |

`CalibrationStatus::Calibrated` is never returned for CTAO profiles because no
dedicated site validation asset is bundled. This is intentional fail-closed
behaviour.

`ValidatedExternalMap` is explicit and remains outside `ComponentMask::ALL`.
Failure of any admission check is an error; it never falls back to the bundled
seed.
