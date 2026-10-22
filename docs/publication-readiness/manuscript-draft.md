# NSB: A Reproducible Rust Framework for Optical Night-Sky Background Modelling and Observation Planning

**Working manuscript v0 — scientific software/methods article. NOT submitted.**
Date of evidence audit: 22 October 2026.
**Author list, affiliations, corresponding author, funding, acknowledgements, conflicts and final release DOI require author approval.**
This working text is not JOSS-formatted `paper.md`, nor does it claim to be a finished journal submission. Values quoted from existing historical experiments are clearly attributed.

## Abstract

Ground-based astronomical observations are affected by night-sky light from multiple celestial and atmospheric sources. Predicting its direction- and time-dependent intensity requires model-specific inputs, atmospheric assumptions and careful treatment of observable quantities. We describe **NSB**, an open-source Rust framework for optical night-sky background estimation and observation planning, with Python and command-line access to the same scientific evaluation engine. NSB combines empirical zodiacal light, atmospheric airglow, scattered moonlight and admitted map-derived integrated starlight where scientifically and legally available. Its main point interface reports photon radiance integrated over 300–650 nm in photons cm^-2 ns^-1 sr^-1, while threshold searches identify candidate observing windows. A separate, domain-restricted spectral interface exposes only components for which the requested calculation is physically supported. The implementation records model provenance, maturity and unavailable uncertainty rather than treating planning configurations as site calibrations. Numerical verification is supported by model-specific regression and analytic/independent reference tests; experimental radiative-transfer solvers remain distinct from the production evaluator. A previously registered conditional comparison with 94 FORS1 spectra on 17 evaluation nights gave mean absolute relative errors of **0.361** for the earlier NSB baseline, **0.171** for nsb2, and **0.152** for a calibration-night median; these historical results demonstrate limitations, not NSB superiority. A later oxygen-line extension improves development diagnostics on the same inspected cohort but lacks independent generalization. We discuss reproducibility, missing physics and the observational-operator requirements for future absolute-accuracy assessment. NSB's present contribution is an auditable scientific-software and numerical-method workflow, **not** a claim of universally calibrated sky predictions.

**Keywords:** night sky background; scientific software; optical radiometry; reproducibility; astronomical observation planning; atmospheric airglow; Rust; Python.

## 1. Introduction

Optical night-sky brightness affects observing conditions and the sensitivity of astronomical instrumentation. Relevant signals include zodiacal sunlight scattered by interplanetary dust, stellar light, nonthermal atmospheric emission and moonlight redirected by the atmosphere. Their relative importance changes with target direction, epoch, solar activity, observing geometry and atmospheric state [1–5]. A physically informed sky-background estimate is therefore a useful input to observing decisions, but it is not equivalent to a detector measurement without a defined atmospheric and instrumental response.

Previous studies established physical models and implementations for these ingredients. Leinert et al. [1] collected zodiacal-light reference data; Noll et al. [2,3] provide atmospheric and Airglow reference work; Jones et al. [4] describe a spectral scattered-Moonlight model; and Roellinghoff, Spencer and Funk [5] developed the IACT-focused **nsb2** framework, including spectral and telescope-response modelling. Reimplementing these equations is not, by itself, a discovery in atmospheric optics.

A reusable alternative can nevertheless be scientifically valuable if it offers a carefully scoped observable, reproducible numerical behavior, transparent model/data provenance and interfaces adapted to observation planning. Here we report the architecture, scientific contracts and verification boundaries of **NSB** at the audited source revision [6]. We explicitly distinguish *implementation correctness*, *physical plausibility* and *independent empirical predictive accuracy*. We compare its intended functionality with nsb2 and preserve negative or blocked observational evidence, rather than claiming that software test coverage establishes astronomical calibration.

## 2. Physical models, quantities and assumptions

### 2.1 Point observable and model composition

The public point query supplies geodetic observing coordinates, UTC time, target ICRS/J2000 right ascension and declination, selected components and an atmosphere/site configuration. NSB's standard integrated quantity is
```text
R_[300,650] = sum of supported selected component photon-radiance integrals
unit: photons cm^-2 ns^-1 sr^-1
```
over 300–650 nm. For a properly matched spectral energy surface radiance `I_lambda`, the photon integral is `R_[a,b] = integral_a^b I_lambda(lambda) * lambda/(h c) d lambda`, with explicit unit conversions. That expression cannot supply an unmeasured source spectrum from a pre-integrated map.

The numerical component-sum identity is a software contract, not a guarantee that all sources share a fully calibrated ground-level or instrument-weighted observable. In particular, the integrated starlight map represents a top-of-atmosphere source that is not automatically attenuated by a production spectral transfer operator. Broadband `B/V` magnitude and S10 outputs use 445/551 nm central-wavelength proxies; they are not synthetic Johnson bandpass photometry [6,7].

### 2.2 Zodiacal light

The configured first-release source is based on Leinert et al. [1] brightness anchors, a solar reference spectrum and wavelength reddening. NSB evaluates the relevant solar/ecliptic geometry and integrates the resulting spectrum. A selectable Noll-style extinction approximation attenuates the celestial source for ground-observer planning; turning it off does not change the source-model identity. Table anchors, formula and unit tests protect numerical semantics. The reference model remains a generic clear-sky approximation with no universal site-specific photometric certification.

### 2.3 Atmospheric airglow

The default airglow model is derived from the continuum of PALACE v1.0 [3], with season, time-of-night, solar F10.7 and emitting-volume geometry. Its Paranal data lineage should not be confused with calibration for every observer site. An explicit experimental variant adds the Og/Or oxygen subset at vacuum wavelengths 557.8887, 630.2046, 636.5536 and 639.35 nm. Selected line strengths use integrated Rayleigh quantities, source-specific emission heights and time/solar factors. The variant omits many other atomic/molecular species and detailed wavelength-dependent absorption; no instrumental line-spread function is reconstructed by inserting delta lines.

The automatic model remains continuum-only; a fitted continuum scale is never asserted to be a calibrated atmosphere simply because it has a checksum, registered provenance or successful synthetic parameter recovery [8]. The PALACE training domain and extrapolations are part of the scientific interpretation.

### 2.4 Stellar light

A data-production workflow constructs checksum-validated Galactic HEALPix products from stellar catalogues, including Gaia-derived sources and a bright-star supplement as applicable to the historical integrated map. The established full-band product has 196,608 HEALPix pixels at NSIDE 128. The admission process attaches source, licences, input selection, flux and scientific comparison evidence. Availability in a release is governed by a registered, approved compact binary plus provenance sidecar, not by automatically trusting a checked-in candidate.

Separate Gaia XP-based wavelength-bin maps integrate **native complete bins** and remain top-of-atmosphere experimental products. A native-bin integral cannot generate an arbitrary partial-band spectrum, a slit magnitude cut, or an atmospheric transfer without additional validated assumptions [6,9]. Accordingly, the restricted-band point API rejects stellar contributions instead of combining incompatible integrals.

### 2.5 Moonlight

The ordinary spectral Moonlight model follows Jones et al. [4], combining lunar irradiance, phase and observing geometry, atmospheric Rayleigh/aerosol scattering and a tabulated multiple-scattering correction. The Krisciunas–Schaefer prescription [10] is exposed as a separate analytic V-band reference. Existing fixtures constrain implementation fidelity to specified inputs, not absolute model accuracy in arbitrary atmospheric conditions. A component that already includes atmospheric scattering must not be passed through another direct-extinction or scattered-light operator without carefully removing the double count.

## 3. Computational architecture and interfaces

NSB is organized into a typed scientific core (`crates/nsb`), a source-installed CLI (`crates/nsb-cli`) and an offline scientific-data preparation/verification tool (`crates/nsb-data-tools`). Rust, Python bindings (PyO3) and CLI access the same underlying runtime rather than implementing three independent astronomical models. Fundamental coordinate/time, atmosphere and quantity primitives are consumed from Siderust, Tempoch and their re-exported optics/units libraries; those underlying capabilities are not presented as newly invented NSB algorithms.

Point evaluation composes configured components and records per-component provenance, scientific status, validated domain and available/unknown uncertainty. A threshold-window query prepares the Sun, astronomical-night, target-visible and Moon-up intervals, subdivides along relevant physical regime boundaries and locates threshold crossings by adaptive exact evaluations and fallback scanning. Its purpose is computational reuse while preserving physical per-sample evaluation; performance does not constitute an accuracy metric.

The experimental transport layer separates `Identity` and Beer–Lambert direct attenuation from source emission, rejecting invalid radiance origins. A separate opt-in plane-parallel scalar single-scattering reference computes outgoing direct and first-order paths with explicit angular and vertical quadrature; higher orders remain absent. An experimental native-bin HEALPix starlight adapter and separately documented higher-order numerical references extend this research layer. These solvers are not silently used by the ordinary evaluator, and their numerical convergence is not an astronomical uncertainty budget [6,9].

Offline-prepared source assets and versioned manifests constrain data versions, checksums and model maturity. Unknown statistical/systematic uncertainty is represented as unavailable instead of zero. Spatial/angular apertures, atmospheric quality and instrumental spectral response require additional transfer information when model output is compared to actual observations.

## 4. Implementation and numerical verification

The repository supplies tests for component summation, input/domain rejection, angular and calendar geometry, constrained band integrals, byte-pinned data reconstruction, moonlight/zodiacal reference identities and selected synthetic numerical limits. Documented independent Scipy/analytic references exercise scattering first-order transport, phase normalization, layer invariance, native-bin wavelength refinement and equal-area HEALPix integration. A spectrally integrated model must not pass a test by replacing an unknown intra-bin shape with a silently assumed center value. These are **reported repository verification resources**, not new numerical results from the present publication audit.

A manuscript release must list exact commands, source+asset SHA-256 identities, third-party dependency versions, OS/compiler and sample numerical outputs from executed clean-install Rust/Python/CLI checks. At the audited commit a root Cargo patch pins an upstream Siderust Git revision; registry publication must wait for the release transition tracked in NSB issue #265. Until that transition is verified, claims of a registry-only reproducible clean installation of version 0.1.1 remain premature.

## 5. Observational evaluation and what it does not establish

### 5.1 Conditional FORS1/Patat reference

An independent `nsb-validation` campaign compared four selected contributions, integrated over 365–650 nm, with flux-calibrated FORS1 spectra from Patat's Paranal library [11]. A preregistered moonless cohort contained 103 selected spectra on 22 nights, divided into 5 baseline-calibration and 17 evaluation nights. The historical held-out evaluation used 94 spectra; model sources were NSB `1b255672...` and nsb2 `bc9320db...`, not the present oxygen-enabled main. Mean absolute relative error was **0.361** for original NSB, **0.171** for nsb2 and **0.152** for the calibration-night median baseline. The paired NSB-minus-nsb2 relative-MAE difference was **0.190** with a 95% night-bootstrap confidence interval of **[0.133, 0.244]**. Thus, nsb2 performed better than the earlier NSB configuration under this study; neither improved meaningfully upon the simple empirical baseline. The missing line-rich Airglow in NSB and differing stellar-population/transport conventions limit physical completeness and generalization [12].

A separate development diagnostic using the same inspected cohort found that opt-in PALACE oxygen and matched observed F10.7 reduced NSB's relative MAE to **0.16573**; nsb2 under matched solar values had **0.17425** and the simple baseline **0.15245**. The paired oxygen–nsb2 relative-MAE confidence interval included zero, and **79/103** cases exceeded PALACE's reported solar training range. These are hypothesis-generating **development scores**, not a new independent validation or evidence of statistically significant superiority [8,13].

### 5.2 Other data and admission limits

The FORS1/FORS2 magnitude campaigns identify many potential archive records but cannot match a complete NSB predicted effective instrument passband/zeropoint to the photometry. The three-night DESI DR1 pilot contains real nominal sky spectra but lacks resolved diffuse calibration and field/line/stellar response equivalence; measured-vs-partial-model differences therefore remain exploratory and are **not accuracy residuals**. LVM DR20 development spectra can be reconstructed nominally, but the SkyW diffuse angular/absolute transfer, shared covariance, observing-condition constraints and complete model-response mapping are not certified. Its registration remains **CONDITIONAL**, and all protected outcomes are sealed. No LVM sky-accuracy number exists [14–16]. Operational closure of LVM is not a reason to withhold an appropriately scoped methods/software description.

## 6. Position relative to nsb2 and existing tools

The published nsb2 model [5] targets IACT spectral background, atmospheric propagation and raytracing-derived telescope/pixel response, whereas NSB emphasizes typed point and observing-window queries, a Rust scientific core, cross-language interfaces, offline verified scientific assets and explicit fail-closed maturity semantics. Both use established astronomical reference literature; some source/calibration data are correlated. NSB is **not claimed to be the first or most accurate** night-sky model, to model telescope counts as nsb2 does, to implement every radiative-transfer process in default point predictions, or to replace observational calibration. Its defensible distinguishing value is the software interface, reproducibility and scientific-contract methodology for this narrower use case.

## 7. Limitations and reproducibility

Current default Airglow omits lines; the oxygen extension covers only a subset of PALACE and has extrapolation/absorption limits. The integrated starlight map does not encode arbitrary wavelength or slit-selected populations and lacks default spectral atmospheric propagation. Broadband proxies are not standard magnitudes; the Moonlight atmosphere and astronomical twilight/foreground components are incompletely tested against independent observations. Generic and CTAO site profiles are not calibrated site-specific predictions. Reference numerical experiments do not establish observational model discrepancy. Missing source/atmosphere/instrument covariance prevents a universal uncertainty estimate. Short local performance measurements are not a cross-model speed ranking.

The exact model/validation commits, raw-data acquisition identities, frozen protocols and complete reexecution recipes are archived in the project repositories [6,12–16]. A final reproducibility appendix must be generated from one tagged clean build and include installed wheel and CLI results. The Gaia-derived assets have source-specific noncommercial and attribution terms, separate from the AGPL software licence.

## 8. Conclusions

NSB packages an auditable, reusable framework for optical night-sky background component evaluation and observing-window planning. Its most substantiated publication contribution is the **scientific software methodology**: typed observable and configuration boundaries, reproducible assets, numerical verification and explicit maturity/unsupported-domain signaling. Historical empirical comparisons disclose meaningful limitations, including inferior performance to nsb2 and to a simple median baseline in a matched conditional FORS experiment. The present software does not support claims of universal absolute predictive accuracy or novel underlying sky-emission physics. A clean tagged release, executed cross-interface verification and independent human review of the evidence and manuscript constitute a limited route to an honest software/methods publication; full calibrated accuracy should be studied separately.

## Data and software availability (working statement)

- NSB source [GitHub](https://github.com/VPRamon/NSB) (audit commit `57584ee298f25f8af7a6f169d71e87f8a59eea9a`).
- Independent evaluation and empirical receipts [nsb-validation](https://github.com/VPRamon/nsb-validation) (main `3191bfc7597b5a2add95456aeb2ea1b906979d22`; separate open [draft #19](https://github.com/VPRamon/nsb-validation/pull/19)).
- Comparison implementation [nsb2](https://github.com/GerritRo/nsb2), pinned `bc9320db03fbff69997ce366c6e76a37339c5d00`.
- Final package archive, DOI, actual public release, build logs and data-licensing statement: **to be inserted only after verification**.

## Declarations to be completed by human authors

**Authors/contributions, funding, conflicts of interest, software/data licence review and acknowledgements:** verify and add before submission. **Generative-AI assistance disclosure:** this initial text and evidence-organization dossier received OpenAI GPT-6 assistance; human authors must verify every claim, edit the manuscript, independently own judgments/experimental choices and state the exact tools and scope required by the chosen venue. No external contact or manuscript submission has been authorized.

## References (working; expand to full journal style)

1. Leinert, C. et al. (1998). The 1997 reference of diffuse night sky brightness. *A&AS* **127**, 1–99.
2. Noll, S. et al. (2012). An atmospheric radiation model for Cerro Paranal. *A&A* **543**, A92.
3. Noll, S., Schmidt, C., Hannawald, P., Kausch, W. & Kimeswenger, S. (2025). PALACE v1.0: Paranal Airglow Line And Continuum Emission model. *GMD* **18**, 4353–4398. https://doi.org/10.5194/gmd-18-4353-2025.
4. Jones, A., Noll, S., Kausch, W., Szyszka, C. & Kimeswenger, S. (2013). An advanced scattered moonlight model for Cerro Paranal. *A&A* **560**, A91. https://doi.org/10.1051/0004-6361/201322433.
5. Roellinghoff, G., Spencer, S. T. & Funk, S. (2025). Advanced modelling of the night sky background light for imaging atmospheric Cherenkov telescopes. *A&A* **698**, A212. https://doi.org/10.1051/0004-6361/202554532.
6. NSB, audited source `57584ee298f25f8af7a6f169d71e87f8a59eea9a`, [scientific model](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/specifications/scientific-model.md).
7. NSB, [scientific metadata contract](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/specifications/scientific-metadata.md).
8. NSB, [oxygen / calibration evidence](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/nsb_components/airglow/validation/oxygen-and-calibration-v1.md).
9. NSB, [starlight transport validation](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/validation/starlight-transport-267.md).
10. Krisciunas, K. & Schaefer, B. E. (1991). A model of the brightness of moonlight. *PASP* **103**, 1033.
11. Patat, F. (2008). The dancing sky: 6 years of night sky observations at Cerro Paranal. *A&A* **481**, 575. https://doi.org/10.1051/0004-6361:20079279.
12. nsb-validation, [four-component Patat comparison](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/patat-fors1-spectral-v1/report.md).
13. nsb-validation, [oxygen development diagnostic](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/airglow-diagnostic-v1.md).
14. nsb-validation, [ESO observational feasibility](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/eso-validation/report.md).
15. nsb-validation, [DESI DR1 pilot](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/desi-dr1-pilot-v1/report.md).
16. nsb-validation, [LVM admission decision](https://github.com/VPRamon/nsb-validation/blob/50053386656628498c04e4e22681352df84624fc/reports/lvm-admission-evidence-v2/report.md) (draft, not merged).
