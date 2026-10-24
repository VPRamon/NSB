# Implementation verification — 24 October 2026

This is a concise engineering/scientific verification receipt, not a replacement
for the immutable generated reports or new scientific admission. Generated data,
figures, wheels, receipts and executed notebooks are excluded from Git.

## Source and environment identities

- NSB clean main source: `f5e1d22689ed417c0dcf9df72da54365c30f5bd5`.
- nsb2 public main and installed PEP 610 identity:
  `bc9320db03fbff69997ce366c6e76a37339c5d00`.
- Source installation exercised the documented Maturin 1.15.0 / `--release --locked`
  route, with rustc 1.97.1, Cargo.lock SHA-256
  `ec909ebb098aa283b97403d943062024cbe18979c8fc5d2d1d023fe837c24b08`,
  Rust Siderust 0.15.1 / Tempoch 0.11.1, Python siderust 0.5.1 / tempoch 0.5.0.
- Build/runtime wheel SHA-256:
  `16ea1fc237d6e40c3f63e7e5ffb4357c0c52c4f1dce6cf605757f2ee5b7a74d2`.
- Imported native extension SHA-256:
  `d0d95cbf98bd8500c93fa2968e31ffc6b289a6a95e0ae2ab1d1ae6560734e6e3`.
- Real-data execution: Python 3.13.14, locked environment with the models extra.
- Separate clean Python 3.12.13 environment: `uv sync --locked --python 3.12`,
  with neither `nsb-py` nor `nsb2` installed; **30 offline tests pass**.
- PyPI JSON on this date reported latest `nsb-py=0.1.1`; source 0.2.0 was built
  locally. Published-package installation of a future Oxygen-capable release
  has not been exercised or asserted available.

## Input and protocol checks

ESO's primary URL returned HTTP 404. The exact registered Wayback fallback
successfully supplied all 18,278,400 bytes with the required full archive hash.
There were 1,189 FITS products. The public selection independently recovered
103 spectra / 22 nights, 94 spectra / 17 evaluation nights, and the same
calibration-night partition and identity checksum.

The full NOAA observed-noontime file matched registered SHA-256
`7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96`.
Every centered 27-day window was complete. Measured solar values agree exactly
with the independent reference; 79/103 spectra are outside 67–166 sfu.

## Complete real-data notebook

All notebook cells executed through nbclient against the receipt-verified current
source build. Both native and matched scenarios evaluated all 103 observations
with all three configurations; **zero missing/rejected predictions**. Statistics
use the registered 94-spectrum evaluation partition. Eight figure families
(14 PNG + 14 SVG files across the two scenarios), Markdown/HTML, machine-readable
results and complete manifest passed artifact verification. The spectral,
solar-sensitivity and model-error figures were visually inspected.

| Configuration | Native relative MAE | Matched measured F10.7 relative MAE |
|---|---:|---:|
| NSB Standard | 0.36102822 | 0.35026902 |
| NSB Oxygen | 0.23882020 | 0.16572597 |
| nsb2 | 0.17086551 | 0.17424942 |
| Calibration-only baseline | 0.15245092 | 0.15245092 |

Matched Oxygen minus nsb2 relative-MAE difference is −0.00852345, with paired
95% night-cluster CI **[−0.05709181, 0.05575557]**. Absolute MAE is higher for
Oxygen by 0.00250069 radiance units (CI [−0.01370266, 0.02409097]). This establishes
neither superiority nor independent post-development validation.

The local delivery report's detached manifest digest was
`ac65d637098ccd380cf65204b2259763999eeac206f755d12983a956b2427de5`.
It records the implementation checkout's then-dirty state and source-file hashes;
that disclosure is retained. It is not a claim to execute a later PR commit.
No generated scientific report is checked into Git.

## Comparison with independent historical evidence

The optional public `reference_check` command was run using local, externally
supplied checksum-pinned benchmark and diagnostic artifacts. These files are
private-access evidence; they are neither distributed nor required for normal
public execution. The private implementation itself was not imported/copied.

- Observation identities, individual night/split assignments: exact.
- Measured per-observation photon radiance maximum difference: `2.78e-16`.
- Standard NSB and nsb2 native component predictions: zero difference.
- Native/matched NSB continuum and Oxygen Airglow: zero difference.
- All six model/scenario total-prediction maximum differences: `1.11e-16`.
- Residual maximum difference: `3.33e-16`.
- New-versus-historical aggregate metric maximum difference: `5.00e-16`.
- Independent rebuilding of historical metrics: maximum difference `1.39e-17`.
- All rebuilt historical paired-bootstrap CI endpoints: zero difference.

The computational tolerance is absolute `1e-12`, justified by roundoff on
identical samples/operators, not physical calibration accuracy. Matching numbers
**do not make different Git commits, dependency graphs or binaries interchangeable**.
Original benchmark `1b2556…582ec`, Oxygen numerical development `168e73…c305`,
later merge `57584e…e9a9a`, and this current source remain distinct identities.
Exact byte-identical historical execution is not claimed; historical mode displays
public development evidence only.

## Other checks and boundaries

Ruff lint/format, tracked Python license headers, `git diff --check`, and
`actionlint` for the new workflow passed. Offline tests cover the notebook schema
and full execution, independent analytic/Astropy unit oracles, checksum refusal,
unsafe TAR/link/duplicate entries, extracted inventory, cut boundaries, matching
IDs, missing/nonfinite predictions, train/evaluation night isolation, deterministic
cluster bootstrap, capability and clean-source rejection, actual import/wheel
receipt verification and tampering, nsb2's template resolver, and manifest integrity.

Existing scientific-reproducibility notebook, core models/defaults, runtime crate,
scientific assets, release workflows and historical evidence remain unchanged.
The full existing Rust suite was not rerun for this examples/documentation-only
change. No live Oxygen-capable PyPI release or byte-identical historical binary is
available through this new public workflow. These are disclosed limits, not
blockers to the successfully executed new observational campaign.
