# Publication strategy, evidence-to-claim map and minimal experiments

Status: provisional PI/reviewer assessment at inspected commits, **not a journal acceptance prediction**.
Recommended decision: **PROMISING — LIMITED WORK REQUIRED**.

## Scientific and methodological contribution

**Central, currently defensible contribution:** *NSB offers a reproducible, typed, component-aware Rust framework for optical night-sky radiance estimation and observing-window planning, with reusable Python/CLI interfaces, explicit science-maturity/uncertainty metadata, pinned offline scientific assets, and domain-aware numerical verification.*

This is primarily a **scientific software / computational-methodology** contribution. The publication does **not** need a claim that NSB is as empirically accurate as nsb2 or is universally calibrated.

Secondary supported/qualified aspects:

1. A documented decomposition of sky emission, atmospheric treatment, component selection, observing geometry and window-threshold evaluation, with physical maturity exposed through outputs.
2. Reference numerical solvers and verifiable spectral-asset contracts, explicitly distinguished from the production full-sky model and their missing physics.
3. A rigorous evaluation methodology that *preserves negative baselines, unsupported observable gates and statistical unknowns*. Cite independent `nsb-validation` results without calling the software itself independently validated.

No claim to have invented Leinert zodiacal emission, PALACE Airglow, Jones scattered Moonlight, HEALPix, basic RT, Siderust astronomical types, or spectroscopic calibration.

## Venue shortlist (official policies reviewed 22 October 2026)

| Venue | Fit and requirement | Important risk / action |
| --- | --- | --- |
| **Astronomy and Computing** (preferred for a fuller **scientific software and methods manuscript**) | [Publisher scope](https://shop.elsevier.com/journals/astronomy-and-computing/2213-1337) explicitly invites software releases, astronomical simulations and reports on practice, including positive/negative results; asks for alternative-work awareness and sustainable source links. | Must articulate meaningful astronomical-research value beyond wrappers, honest nsb2 differentiation and numerical/observational evidence. Check the current guide, article types and fees when actually submitting. |
| **JOSS** (best concise software-publication alternative **only if pre-review gates pass**) | [Official 2026 instructions](https://joss.readthedocs.io/en/latest/submitting.html): OSI licence, feature-complete distributable software, `paper.md`, tests/docs, concrete research usage/impact and statement of need; >6 months public development and sustained activity. | Verify NSB's actual public history and research adoption, clean-installable 0.1.1, durable external usability and code novelty relative to nsb2. JOSS paper **must not center new science results**. Required sections include state of field, software design, impact, AI-usage disclosure. AI-assisted text must be human-reviewed and disclosed. |
| **AAS Journals** (domain-specific software article) | [Software policy](https://journals.aas.org/policy-statement-on-software/) welcomes astronomical software descriptions of design, novel features and intended use **without requiring new scientific research results**. | Need credible distinct features, reproducibility, astronomy users and target-journal match; actual manuscript rules require a separate check. |
| **SoftwareX** (software-focused fallback) | [Publisher description](https://www.elsevier.com/researcher/author/tools-and-resources/research-elements-journals): peer-reviewed open-access description of research software and applications. | Check current template, open-access charges/licensing and application evidence before deciding. |

A research-results-focused A&A or novel theoretical-RT paper would currently demand a distinctly **new** verified astrophysical/mathematical result, independent observational inference or methodological benchmark beyond what has been established. Do not target that first by overstating novelty.

## Candidate manuscript titles

1. **NSB: A Reproducible Rust Framework for Optical Night-Sky Background Modelling and Observation Planning** (recommended)
2. **Component-Aware Night-Sky Radiance Evaluation with Explicit Scientific Provenance in Rust and Python**
3. **NSB: Verified Software Methods and Validation Boundaries for Ground-Based Night-Sky Background Estimation**

## Claim-to-evidence plan

| Manuscript claim | Primary artifact and permitted interpretation | Figure/table |
| --- | --- | --- |
| Point and window interfaces serve a scientific observing use case | [API and planning description](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9/docs/specifications/scientific-model.md), [`evaluator/point.rs`](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9/crates/nsb/src/evaluator/point.rs), CLI/Rust/Python examples | Fig. 1 architecture + a **fresh** deterministic time/direction example |
| Correct numerical units/component composition and domain gates | [validation matrix](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/specifications/validation.md), [band code](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/crates/nsb/src/band.rs), [metadata](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/specifications/scientific-metadata.md) | Table 1 physical contracts, Table 2 verification |
| Numerical RT references possess independent/synthetic checks | [single-scattering spec](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/specifications/single-scattering.md), [starlight validation](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/validation/starlight-transport-267.md), [higher-order tests](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/validation/multiple-scattering-269.md) | Fig. 2 resolution/ref oracle, **label experimental** |
| Observational comparison is conditional and includes unfavorable results | [Patat report](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/patat-fors1-spectral-v1/report.md) and [oxygen development diagnostic](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/airglow-diagnostic-v1.md) | Fig. 3 original relative MAE with CI incl. baseline; optional separated development inset |
| Published interfaces do not imply universally calibrated observational predictions | [model maturity](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/specifications/model-maturity.md), [DESI pilot](https://github.com/VPRamon/nsb-validation/blob/3191bfc7597b5a2add95456aeb2ea1b906979d22/reports/desi-dr1-pilot-v1/report.md), [LVM decision](https://github.com/VPRamon/nsb-validation/blob/50053386656628498c04e4e22681352df84624fc/reports/lvm-admission-evidence-v2/report.md) | Table 3 admitted / conditional / exploratory / blocked evidence |
| Reproducible release usage | `Cargo.lock`, data manifests, [release workflow](https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/.github/workflows/publish.yml), executed new clean-install and parity receipts (currently **missing**) | Table 4 commands, exact artifacts + SHA and output parity |

**Do not add a publication figure for unsupported absolute LVM residuals.** Existing original plots remain in validation, immutable, with source identities; do not revise registered metrics to make graphics prettier.

## Value / effort: only targeted additional experiments

### E1. Clean-release replay and Rust/Python/CLI equality — **essential; highest value/low effort**

- **Question / claim:** Can a non-developer install the exact release and obtain reproducible numerically equivalent science on each public interface?
- **Sources:** NSB code + approved asset SHA; release Git tag/source distribution, pinned Cargo/dependency environment; same immutable point fixture (e.g. Paranal, a moonless target and UTC) for Python/Rust/CLI.
- **Observable:** integrated radiance in ph cm^-2 ns^-1 sr^-1; per-component fields, flags/maturity, selected model, F10.7 provenance, supported restricted bands. Output and environment checksums.
- **Preregistered criterion:** zero wrong-quantity/component, invalid-domain or provenance mismatches; floating-point equality within explicitly recorded tolerance after demonstrating a common evaluator configuration, not a chosen post-hoc threshold. Packaging complete with released immutable Siderust dependency; clean Cargo + isolated Python wheel success; CLI installation reproducible.
- **Execution / cost:** local CI job + clean separate environment, 1–3 hours excluding upstream release gating. Existing tests should be **reused**; no redundant suite invented. `cargo test --workspace --locked`, `cargo package -p nsb --locked`, built-wheel pytest, CLI JSON fixture.
- **Confounders:** site profiles and default assets may differ across releases; PyO3 Siderust compatibility; time scale/data-file identities, serializer precision.
- **If negative:** reproducible software claim must be reduced; fix release-blocking packaging or inconsistent configuration before submission.

### E2. Independent numerical oracle receipt for the exact release — **essential as verification receipt; low/medium effort**

- **Question / claim:** Do the current source/transport and numerical integrals satisfy independent analytic limits and converge without accidental tolerance cancellation?
- **Sources:** existing independent SciPy and HEALPix reference generators, analytical Beer–Lambert and single-scatter homogeneous/equal-cosine limits, bounded wavelength integrals and existing pinned fixtures.
- **Observable:** direct/single radiance ph cm^-2 ns^-1 sr^-1 nm^-1 for experimental solver, 300–650 nm radiance sums and native 360–570 nm bins; dimensionless relative differences, refinement order and error signals.
- **Preregistered criterion:** **use already registered test tolerances and source identities** from the corresponding spec; report each reached refinement and independently computed result. No new error budget inferred from tolerance. Fail unsupported partial bins and invalid atmosphere origins.
- **Execution / cost:** run existing offline fixtures at tagged version, collect concise machine-readable receipt; not a new RT solver or broad campaign.
- **Confounders:** changed upstream Siderust optics, data-version drift, floating-point platform, actual solver not connected to evaluator default.
- **If negative:** fix affected numerical bug or remove numerical-precision claim for that surface; do not advertise experimental solver as production.

### E3. Reproducible model-domain and planning example — **essential to a useful software article; low effort**

- **Question / claim:** Are site/time/direction inputs, component choices and threshold windows inspectable, with explicit incomplete-model warnings?
- **Sources:** main Rust/Python/CLI examples; same typed UTC epoch, observer, target; checksums and clean runtime. Two configurations: default continuum and explicit experimental oxygen (only if its domain is satisfied).
- **Observable:** 300–650 nm integrated ph cm^-2 ns^-1 sr^-1, contributions, window boundaries in UTC seconds and metadata.
- **Criterion:** exact supported/domain checks, nonempty provenance, reproducible numeric outputs across interfaces; no demonstration of predictive accuracy. Provide numerical output **only after execution**.
- **Cost:** under a day incl. plotting one understandable non-misleading figure; existing examples may suffice.
- **Confounders:** executable tag, sunset calculations, site profile, dependencies and solar activity selector.
- **If negative:** narrow documented use cases, correct schema or report physically unsupported configuration.

### E4. Historical Patat scores provenance/replay — **optional but valuable; low effort if originals available**

- **Question / claim:** Do preserved frozen reports/figures still match their published checksums and registered model builds?
- **Sources:** `nsb-validation` historical Patat protocol, manifest and cached public raw spectra; no new evaluation selection or fit.
- **Observable:** rel-MAE, MAE, RMSE, night-bootstrap CI and baseline under original 365–650 nm operator.
- **Criterion:** byte/checksum or declared numerical tolerance identity with **original** run. Any changed result becomes a distinct run, not a revision of frozen evidence.
- **Cost:** offline manifest check first; full-data replay only if retained inputs locally available and permitted. Not needed to assert that repository previously reported these figures.
- **Confounders:** archived web host, old NSB binary, original manifest self-entry erratum (later documented).
- **If negative:** remove or clearly label nonreproducible report evidence until discrepancies are traced.

### E5. Independent new absolute observational calibration — **NOT essential for proposed paper; very high effort**

- **Question:** What is calibrated external predictive accuracy after matching the full sky/aperture/LSF/atmosphere observable?
- **Sources:** future independently calibrated, uninspected datasets with signed protocols.
- **Observable:** exact matched ground radiance or effective passband with full uncertainty/covariance.
- **Criterion:** admission and error metrics fully preregistered, independent model/data lineage; protected holdouts untouched.
- **Cost:** research project, not a release-blocking software-paper task.
- **If negative:** report true predictive limitations in a separate empirical study. Do not reopen LVM without listed technical evidence.

## Submission blockers versus discretionary enhancements

**Essential:** close Siderust #265 with release/archive reproducibility; review actual redistribution rights and tagged manifests for the claimed distribution; execute E1/E2/E3 on exact manuscript code tag; verify public-history/impact criteria if targeting JOSS; accurately cite model sources and nsb2; human review of paper/science figures, license and AI-disclosure statements. Exact user-facing outputs and hash-backed metadata must match selected tag.

**Optional:** more extensive first/higher scattering benchmarks, alternative site validation, full line-family PALACE, DGL, instrument response, independent stellar slit source filtering, calibrated new observations, performance comparison vs nsb2. These are valuable *future research* but should not be required to substantiate a modest software/methodology publication.

## Minimal roadmap (ordered; stop when claims are supported)

1. **Freeze release/artifact identity**: reconcile science docs with actual bundled/external Starlight pair and redistribution consent; upstream Siderust release and remove/replace Git-only patch; tag one reproducible NSB source revision. Acceptance: published/locally verified archive and complete source/data licence notice.
2. **Run existing numerical tests and exact clean-install interop**: E1/E2; record OS/compiler, lock, wheels, hashes, command exit codes, actual output numeric values and independent expected values. Acceptance: no observed failures, no unexplained discrepancy, deterministic declared domain.
3. **Produce one concise methods demonstration**: E3 with text/figure and explicit observational caveats. Acceptance: figure and table generated from pinned runnable script, labelled as synthetic/model outputs.
4. **Prepare manuscript**: migrate [manuscript-draft.md](manuscript-draft.md) to target journal style, insert verified run receipts/results, full references/author affiliations and disclosures; preserve Patat negative result and restrictions. Acceptance: every abstract number tied to an executed reference, source or pin.
5. **Human scientific review**: independent collaborator or appropriately skeptical reviewer checks novelty, uncertainty, source licensing, frozen-validation comparisons and journal rules. Submit **only with explicit author authorization**.

## Final research verdict

**PROMISING — LIMITED WORK REQUIRED**, for a **scientific software and computational-methods paper** (Option A with publication separation of Option C). There is a definite reusable research/software utility, documented model boundaries, numerical-reference evidence, and an unusually candid negative observational benchmark. There is **not** proof of a novel physical theory, universal absolute accuracy, superior accuracy over nsb2, or calibrated observing sites. Low-cost release/verification receipts and a source-checked manuscript are the shortest credible route; a new LVM campaign is not.
