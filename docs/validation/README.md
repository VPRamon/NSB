# Validation reports and numerical evidence

This directory retains independently generated numeric references and
provenance of particular evaluated model revisions. A dated report or one
identified by a PR/commit describes **that evaluated revision** and does not
claim accuracy for a newer software release.

For current scientific use, start with
[scientific validation and limitations](../scientific-validation.md) and the
[validation matrix](../specifications/validation.md). These pages distinguish
implementation checks, cross-implementation agreement and observational
validation of NSB 0.2.0.

## Evidence retained for traceability

- [Starlight transport](starlight-transport-267.md): independently refined
  synthetic spectral and angular integration.
- [Multiple scattering](multiple-scattering-269.md): numerical references
  for a bounded experimental physical configuration.
- [Gaia XP factorized experiments](gaia-xp-factorized.md): independent
  computational comparisons and explicit scientific limits.
- [Gaia XP streaming experiment](gaia-xp-streaming.md): previous partial
  measurements retained for reproducibility, not current performance claims.
- [Spectral window diagnostics](spectral-window-diagnostics-v1.md): Outcome B
  restricted continuum-subtracted Oxygen-window comparisons against NSB
  Oxygen-only integrals; full total-sky spectral MAE rejected
  ([#296](https://github.com/VPRamon/NSB/issues/296)).

Exact third-party producer identities and source hashes in fixtures are
necessary provenance, not NSB runtime dependencies. Do not delete or reword
checksum-pinned approval evidence without renewed verification or approval.
