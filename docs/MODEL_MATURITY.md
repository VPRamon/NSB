# Model maturity

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
| Caller-provided starlight map | Caller-defined | Provenance supplied by caller | Only as justified by caller data |
| B/V S10 and magnitudes | Proxy diagnostic | 445/551 nm central-wavelength convention | No passband-photometry claim |

`CalibrationStatus::Calibrated` is never returned for CTAO profiles because no
dedicated site validation asset is bundled. This is intentional fail-closed
behaviour.
