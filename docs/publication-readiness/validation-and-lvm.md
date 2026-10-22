# Observational evidence, NSB–nsb2 comparison and LVM closure

Status: 22 October 2026; no new observations acquired, no protected records opened and no model values recomputed.
All numerical results below are **historical reports**, not rerun during this audit. Freeze/pin identities before manuscript production.

## Comparable evidence hierarchy

| Campaign | Actually measured and comparable? | Quantitative outcome | Admission / interpretation |
| --- | --- | --- | --- |
| FORS1/Patat 2008 spectrum | 365–650 nm band-integrated ground photon radiance, under stated model+aperture limitations; 103 selected spectra/22 nights, 94 held-out spectra/17 nights | NSB original rel-MAE **0.361** (95% night CI 0.332–0.395), nsb2 **0.171** (0.135–0.210), calibration-night median baseline **0.152** (0.106–0.223); paired NSB−nsb2 **+0.190**, CI [0.133,0.244]. Radiance MAE: 0.121/0.054/0.052 photons cm^-2 ns^-1 sr^-1. | **Conditional empirical comparison**, single site/configuration, incomplete NSB Airglow lines and stellar-population/atmospheric assumptions; **nsb2 performs better than original NSB**, neither beats naive baseline. [Report](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/patat-fors1-spectral-v1/report.md). |
| Oxygen amendment using same Patat data | Adds PALACE Og/Or source lines, holds other predictions constant, matched observed 27-day F10.7 | Relative MAE original native **0.36103**, oxygen native **0.23882**, oxygen matched F10.7 **0.16573**, nsb2 matched **0.17425**, simple baseline **0.15245**. Paired oxygen–nsb2 rel-MAE **−0.00852**; 95% CI [−0.05709,+0.05576]. | **Development only**: comparison nights were already inspected, 79/103 F10.7 conditions above PALACE 67–166 sfu range; does not establish independent generalization, superiority or isolated continuum calibration. [Diagnostic](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/airglow-diagnostic-v1.md). |
| ESO FORS1/FORS2 broadband photometry | Archive metadata and measured calibrated magnitudes; complete NSB synthetic effective-passband observable unavailable | 9,509 FORS2 and 1,780 FORS1 archive observations; candidate metadata cohorts and baseline-only statistics; **zero admitted model comparisons**. | Out-of-band filter leakage, missing full response/zeropoint and modeled SED prevent science accuracy scoring. [Report](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/eso-validation/report.md). |
| DESI DR1 pilot | Three real independently selected nights, nominal 360–570 nm seven-band spectral integrations | Observed totals: 0.19277219, 0.17966123, 0.20844594; NSB restricted Zodiacal+continuum Airglow proxies: 0.09968363, 0.09984468, 0.13361901 (ph cm^-2 ns^-1 sr^-1). | All **exploratory_only**; missing line/Starlight physics, diffuse calibration/sky-fit provenance, angular weighting and shared uncertainty. No admitted bias/MAE/CI. [Report](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/desi-dr1-pilot-v1/report.md). |
| DESI XP/DISORT scattered-light numerical diagnostic | Independent numerical solution of matched transfer idealization, **not** sky measurement | Historical independent scattered integral 0.003086121304126204 vs NSB 0.003086734453522263, ~0.01987% difference (ph cm^-2 ns^-1 sr^-1); other newer transport checks in NSB docs | Numerical/method evidence only, no physical systematic uncertainty or observational calibration. [Readiness report](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/observational-readiness-v1/report.md). |
| LVM SDSS-V DR20 | Original fiber products reconstructed for 5 development exposures/4 nights | Nominal 400–650 nm radiance and spectral/oxygen extraction feasibility. **0 admitted observations, 0 model residuals, no absolute accuracy statistics**. | Admission **CONDITIONAL, not authorized** due to instrumental and observable gates; protected cohort sealed. [PR17](https://github.com/VPRamon/nsb-validation/pull/17), [PR18](https://github.com/VPRamon/nsb-validation/pull/18), [PR19 draft](https://github.com/VPRamon/nsb-validation/pull/19). |

Absolute calibration uncertainty is **not** interchangeable with night bootstrap sampling error. A shared assumed 5% reference calibration scale in the Patat study does not certify model completeness or release calibration. Agreement between models sharing Leinert, solar, Paranal/PALACE/SkyCalc lineage does not constitute independent validation.

## nsb2 is a peer prior art, not a strawman

Comparison code frozen at [nsb2 `bc9320db`](https://github.com/GerritRo/nsb2/tree/bc9320db03fbff69997ce366c6e76a37339c5d00); literature [Roellinghoff, Spencer & Funk (2025)](https://doi.org/10.1051/0004-6361/202554532).

| Dimension | NSB current inspected main | nsb2 inspected main | Meaning |
| --- | --- | --- | --- |
| Scientific goal | Typed optical sky-background point evaluation and threshold observing-window planning across sites; generic clear sky | IACT-oriented wavelength-resolved NSB sources, atmosphere and raytracing-based telescope/pixel response | Distinct use cases; NSB is not a replacement for pixel instrument simulation |
| Runtime | Rust `nsb`, PyO3 `nsb-py`, source-installed CLI, offline data tools | Python/Astropy, raytracing-derived instrument configuration; ctapipe integration | Interoperability / engineering difference, not accuracy |
| Zodiacal | Leinert 1998 + Noll-style approximate extinction and analytical solar spectrum | Leinert tables and spectral sources | Shared established literature, no unilateral novelty |
| Airglow | PALACE continuum by default; explicit experimental selected O lines | SkyCalc line+continuum 130-sfu source rescaled with solar activity / van Rhijn | Historically more complete spectral line support in nsb2; differing source and F10.7 conventions |
| Starlight | Approved/bundled integrated 300–650 HEALPix product, independent experimental XP-native bins; TOA map in production | Gaia faint-star HEALPix/spectral treatment, telescope response | Distinct populations/SED/aperture; direct model-number agreement not equivalence |
| Moonlight | Jones 2013 spectral and KS91 analytic reference | Noll/ROLO-derived lunar scattered light with atmosphere | Models and internal paths differ, neither is globally certified against NSB |
| Atmosphere | Component-specific paths plus separately opt-in typed direct/single/higher-order references, Siderust basis | Single scattering/Rayleigh/Mie/absorption with instrument response | NSB experimental solver must not be represented as `NsbEvaluator` default |
| Bands/units | Integrated radiance 300–650; restricted 300–650 only selected components; 445/551 nm B/V proxies, optional `None` uncertainties | Spectral grids, telescope/pixel response | Only compare identical physical observables after explicit transfer/masks |
| Scientific maturity/repro | Fail-closed checksum/licence/admission, component status, model/source uncertainty metadata, deterministic assets | Python tests, docs, bundled reference data/IACT prior observational comparison | NSB has plausible methodological differentiation, not proof of stronger empirical accuracy |
| Performance | Some local NSB Criterion measurements exist | No fair, jointly pinned, common-observable benchmark reviewed | **No speed ranking** |

The published nsb2 paper reports instrument-response and per-pixel work with H.E.S.S.; do not extrapolate those published observational performance figures to the code-only FORS comparison or to NSB's uncalibrated site profiles. Its paper includes DGL in the research pipeline but DGL is not established as production code in the inspected nsb2 release. Check paper/code boundaries before making component-coverage claims.

## LVM scientific closure memo

### Checked PR identities and scope

- [#17](https://github.com/VPRamon/nsb-validation/pull/17) MERGED; head `d3563e85b5c9cc924e4c72fced441948d7ed32bf`. Original SDSS-V/LVM sky products for 3 independent nights (5355, 5522, 11937) verified, ~1.54 GB outside Git; nominal flux reconstruction. Registered 5 development exposures (4 nights), 63 protected candidates (20 nights), 101 exclusions out of 169 records, no protected opens. [Report](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/independent-observations-v1/report.md).
- [#18](https://github.com/VPRamon/nsb-validation/pull/18) MERGED; head `f2233e0577ff7826a6fa3573371e2a7a405cb4f5`, squash main `3191bfc7597b5a2add95456aeb2ea1b906979d22`. Adds two remaining development observations, verifies DRP calibrations, wavelength/uncertainty and LSF/response issues, nonadmission criteria. [Report](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/lvm-absolute-admission-v1/report.md).
- [#19](https://github.com/VPRamon/nsb-validation/pull/19) **OPEN DRAFT, NOT MERGED** at head `50053386656628498c04e4e22681352df84624fc`. [Provisional report](https://github.com/VPRamon/nsb-validation/blob/50053386656628498c04e4e22681352df84624fc/reports/lvm-admission-evidence-v2/report.md) adds public master-family provenance, development-header timing, nominal oxygen-window extraction (13/15 fully supported windows), X-shooter metadata screening. The two incomplete 636.4 nm windows remain null. Nothing sent to collaborators.

### Decision

**CONDITIONAL / REQUEST_EXTERNAL_EVIDENCE. Treat LVM as an operationally paused route, not a prerequisite for a software/methods paper.** Absolute diffuse SkyW effective throughput/beam/FRD, reference sensitivity and shared epoch covariance, DRP/runtime/master receipts, observer-atmosphere transparency and aerosol/cloud conditions, and sky-vs-model response/stellar-aperture equivalence remain unresolved. The same-response air/vacuum photon integral correction (~+0.028%) is distinct from changing the physical top-hat response (~−0.21% to −0.29% in assessed examples). Fixing wavelength coordinates does not certify calibration.

**Reusable evidence:** verified public exposure identities and hashes, source reconstruction algebra, Jacobian + tested Ciddor inversion, finite-support mask-respecting line extractor, calibration provenance ledgers, 5 development observations, protected cohort protocol, X-shooter preflight, executed validation/reproducibility receipts. These are *methodological* results and may be mentioned in limitations / reproducibility; they are **not** NSB accuracy results.

**Reopen only if ALL relevant prerequisites are met**: (1) externally verifiable exact DR20 reducing code and SCI mean sensitivity inputs/runtime, (2) applicable measured uncertainty/covariance of SkyW diffuse absolute transfer including angular acceptance and chromatic response, (3) matched atmospheric quality and time integration, (4) separately defined and preregistered instrument operator/observable (including masked LSF and starlight/foreground treatment), (5) fully independent calibration and model-source lineage, and (6) acceptance/error analysis frozen **before** opening protected outcomes. Start with a minimal *development* assessment, never the 63 protected spectra. If key metrology is unavailable, close the proposed absolute route as out-of-scope rather than download further sky frames.

The prepared human inquiry is unsubmitted; **do not contact SDSS**, acquire X-shooter or LVM data, modify frozen registrations, merge/close #19 or unseal protected observations as a consequence of this dossier.

## What should appear in a scientific paper

- Main validation figure: original conditional Patat paired results **including baseline**, CI and provenance, with historical-version labels.
- Secondary development figure: oxygen variants **clearly visually and textually separated** from untouched evaluation; include extrapolation warning.
- Blocked-evidence table: FORS magnitudes, DESI, LVM, which operator or calibration makes scoring invalid; keep accuracy cells `not estimable`, never zero.
- Do not label overlapping conditional campaigns as multiple independent positive validations or cherry-pick model configurations.

Proposed future confirmation experiments are in [publication-strategy.md](publication-strategy.md). The protected LVM cohort is **not** among them.
