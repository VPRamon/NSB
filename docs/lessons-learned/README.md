# NSB lessons learned

Status: Project decision records and recurring-error guardrails.
Audience: Contributors, maintainers and scientific reviewers.
Scope: Evidence-backed mistakes to avoid, negative results, and criteria for revisiting decisions.

This is NSB's institutional knowledge base. Consult it **before** reopening an old model, data-policy, or validation approach. An inconclusive or negative result is worth retaining when it prevents a repeated unsupported change.

## Find the lesson you need

| Subsystem | Failure mode to avoid | Guardrail / decision | Investigation |
| --- | --- | --- | --- |
| Solar activity / Airglow | Treating retrospectively measured F10.7 or later-published monthly averages as causal; mistaking out-of-domain extrapolation for validated improvement | Preserve the F10.7 resolver default; disclose time availability and training-domain coverage | [F10.7 temporal cadence](solar-activity-f107-cadence.md) · [Numerical evidence](artifacts/solar-activity-f107-cadence-evidence.json) · [Issue #295](https://github.com/VPRamon/NSB/issues/295) |
| Starlight / cross-implementation | Treating NSB full-population 300–650 maps vs nsb2 G>15 Pickles maps (or solar-colour proxies) as equivalent accuracy scores | Require a frozen population/operator contract; mark unsupported XP source-ID and proxy comparisons | [Matched Starlight NSB↔nsb2](../validation/matched-starlight-nsb2-v1.md) · [Artifact](../validation/artifacts/matched-starlight-nsb2-v1.json) · [Issue #298](https://github.com/VPRamon/NSB/issues/298) |
| Spectral validation / Oxygen | Publishing full spectral MAE of total-sky observations against Airglow-only (or unconvolved delta-line peak heights); treating continuum-control bands as Airglow residuals; isolating nsb2 Oxygen without a separable channel | Restrict to continuum-subtracted Oxygen-window excess vs NSB Oxygen-only; mark unsupported comparisons explicitly | [Spectral window diagnostics](../validation/spectral-window-diagnostics-v1.md) · [Artifact](../validation/artifacts/spectral-window-diagnostics-v1.json) · [Issue #296](https://github.com/VPRamon/NSB/issues/296) |

## Before making a related change

1. Search for the affected subsystem and read the specific mistake and applicability limits.
2. Read the experiment and its evidence rather than relying on a headline metric.
3. Verify whether new inputs were genuinely available at the evaluation instant and whether validation is independent and in-domain.
4. If new evidence changes the decision, amend the entry with source/checksum, statistical uncertainty, and explicit reopening conditions. Preserve historical limitations; do not silently rewrite the old conclusion.

## Adding a lesson

Use a descriptive **topic-first** filename under this directory, such as `solar-activity-f107-cadence.md`. Store compact machine-readable results under `artifacts/` with the same filename stem. Add it to the table above and link it from the owning scientific or technical contract.

Each report should record **mistake to avoid; original hypothesis; actual runtime behaviour; considered alternatives; input identities and checksums; observed evidence and uncertainty; decision; limits and reproducibility; conditions for reopening**. Distinguish development, independent validation and diagnostic results. Cite scientific sources and repository issues; the material is maintained as NSB's documentation.

See also [scientific validation](../scientific-validation.md), [validation reports](../validation/README.md), and the [documentation hub](../README.md).
