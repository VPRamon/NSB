# NSB publication-readiness dossier

Status: **PROMISING — LIMITED WORK REQUIRED** for a **scientific-software / computational-methods** manuscript.
Audited 22 October 2026. This is a **review draft**, not a submitted manuscript, calibration certificate, executed test log, or authorization to access protected observations.

## Executive assessment

The defensible current contribution is a typed, component-aware optical night-sky radiance and observing-window framework with Rust, Python and CLI interfaces, explicit model maturity/uncertainty, checksum-controlled scientific assets and documented numerical verification. It does not establish new sky-emission physics, universally calibrated accuracy or performance superiority over **nsb2**, which is an established IACT-focused spectral/instrument model.

The historical conditional FORS1/Patat 365–650 nm comparison (**94 evaluation spectra from 17 nights**) reported relative MAE **0.361 NSB**, **0.171 nsb2**, **0.152 simple calibration-night median**. The original NSB was worse than nsb2 and neither model improved on the simple baseline. A subsequent oxygen+observed F10.7 development diagnostic reduced NSB error to **0.16573**, but reused already-inspected observations, did not beat the baseline and did not establish independent superiority; 79/103 solar inputs exceeded the reported PALACE training range.

LVM remains **CONDITIONAL**: absolute SkyW diffuse throughput/calibration and covariance plus complete observational equivalence are missing. PR #17/#18 are merged. [nsb-validation PR #19](https://github.com/VPRamon/nsb-validation/pull/19) is **open draft**, not merged and must be left unchanged. All 63 protected candidate spectra remain sealed. LVM is not a prerequisite for an appropriately scoped software paper.

## Files

1. [Scientific audit](scientific-audit.md) — physical models, units, coordinate/atmosphere boundaries, starlight and oxygen status, dependency provenance, numerical methods, claim-by-claim evidence matrix.
2. [Observational validation, nsb2 comparison and LVM closure](validation-and-lvm.md) — Patat/baseline with negative results, oxygen development, DESI/ESO blocked comparisons, independent model differences, LVM reopening conditions.
3. [Publication strategy and experiment priorities](publication-strategy.md) — recommended venue, alternative venues and their publication gates, publication claim-to-evidence/figure plan, E1–E5 test specifications, blockers vs enhancements and ordered submission roadmap.
4. [Working full manuscript draft](manuscript-draft.md) — title, structured article sections, evidence-grounded abstract and results, data/software availability, references and required author disclosures.

## Source versions

| Repository / experiment | Frozen reference |
| --- | --- |
| [NSB](https://github.com/VPRamon/NSB) | [`57584ee298f25f8af7a6f169d71e87f8a59eea9a`](https://github.com/VPRamon/NSB/commit/57584ee298f25f8af7a6f169d71e87f8a59eea9a) |
| [nsb-validation](https://github.com/VPRamon/nsb-validation) | [`3191bfc7597b5a2add95456aeb2ea1b906979d22`](https://github.com/VPRamon/nsb-validation/commit/3191bfc7597b5a2add95456aeb2ea1b906979d22) |
| [nsb-validation PR #19, separate draft](https://github.com/VPRamon/nsb-validation/pull/19) | [`50053386656628498c04e4e22681352df84624fc`](https://github.com/VPRamon/nsb-validation/commit/50053386656628498c04e4e22681352df84624fc) |
| [nsb2](https://github.com/GerritRo/nsb2) | [`bc9320db03fbff69997ce366c6e76a37339c5d00`](https://github.com/GerritRo/nsb2/commit/bc9320db03fbff69997ce366c6e76a37339c5d00) |
| Historical FORS benchmark NSB | `1b2556727205694c7765ef0d254c5d1b301582ec` |
| Oxygen development numerical source | `168e73dae89060423eebc344871b0084d217c305` |

The current code does not become a historical benchmark's code simply because it is more recent. Numerical test coverage, documented fixtures, good provenance and software release readiness are distinct from independently demonstrated physical accuracy.

## Submission gate

**Minimal route:** verify an actually reproducible tagged release and approved redistributable assets (especially the upstream Siderust Git patch / [NSB #265](https://github.com/VPRamon/NSB/issues/265)); execute the existing numerical suite and installed Rust/Python/CLI parity checks against that exact tag; generate a deterministic documented scientific example; then complete journal-specific author, scientific, reference and data/licensing review. An independent absolute sky-calibration campaign is **not** required for the proposed software/methods claims.

**Audit boundaries:** this dossier was prepared through inspected source code, reports and online journal policies, **without running Cargo/Python numerical suites or any new physical/observational experiment**. Reproduced values are historical *reported* results and must not be confused with new independent computations. No LVM protected data were opened, no calibration was fitted and no frozen validation outputs were rewritten. Before submission, humans must verify scientific correctness, bibliography, citations, AI-assistance disclosure, journal rules and source/data redistribution rights.

A new physical-accuracy study should remain independent and use an admitted calibration/measurement operator with frozen cohorts; it must not consume protected LVM evidence on the basis of this dossier.
