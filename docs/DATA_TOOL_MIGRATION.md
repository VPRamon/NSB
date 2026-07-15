# NSB data-tool migration

Issue #58 replaced the historical phase-oriented executable surface with a
smaller capability-oriented toolset. This document records the disposition of
every removed command so old notebooks, reports and release notes can be
interpreted without reintroducing obsolete binaries.

The normative current inventory is
[`crates/nsb-data-tools/tool-registry.toml`](../crates/nsb-data-tools/tool-registry.toml).
Historical scientific evidence and frozen fixtures remain valid; only the
one-shot executable entry points were removed.

## Removed Rust commands

| Removed command | Disposition |
| --- | --- |
| `prepare_starlight_phase5` | One-shot preparation step removed. Reusable sampling, catalogue and policy logic remains in library modules and durable catalogue/sampling commands. |
| `download_xp_continuous_phase5` | Phase-specific target downloader removed. Use `download_gaia_xp_continuous_bulk` for official bulk acquisition or `generate_gaia_starlight_release_inputs` for the controlled release-input workflow. |
| `run_starlight_phase5_overlap_validation` | One-shot validation runner removed. Frozen results remain scientific evidence; durable reconstruction/map validation uses `validate_xp_continuous_reconstruction` and `validate_starlight_map`. |
| `emit_phase5_continuous_contributions` | Phase-specific exporter removed. Integrated product generation must consume typed, validated contributions through `build_integrated_starlight_product`. |
| `finalize_starlight_phase5` | One-shot finalizer removed. Reconciliation and admission belong to the durable generation/validation services. |
| `inspect_phase5_download` | Phase-specific inspector removed. Acquisition commands own their versioned manifests, diagnostics and resume state. |
| `run_phase5b_cross_comparison` | Development comparison removed. Relevant parity evidence belongs in automated fixtures/tests and `validate_xp_continuous_reconstruction`. |
| `run_phase5b_mini_pilot` | Development pilot removed. It must not be used as a production bulk orchestrator. Reusable parsers and HEALPix accumulation remain library code. |
| `run_phase5b_chunk_benchmark` | Ad hoc benchmark executable removed. Performance evidence belongs in the benchmark/test harness tracked by #60. |
| `run_phase5b_merge_validation` | Manual merge probe removed. Merge determinism belongs in automated integration tests tracked by #60. |
| `run_phase5b_resume_validation` | Manual resume probe removed. Crash/resume behavior belongs in automated fault-injection tests tracked by #60. |
| `run_phase5b_multifile_pilot` | Development orchestrator removed. A future production orchestrator must expose a durable capability and typed state model rather than a phase name. |
| `archive_phase5_policy_v0` | One-off archival command removed. The archived policy evidence remains immutable documentation/data. |
| `freeze_phase5_validation_policy_v1` | One-off policy-freeze command removed after policy v1 was frozen. The approved policy file and checksum remain the source of truth. |
| `prepare_phase5_holdout_v1` | One-off holdout preparation removed. The frozen holdout definition and evidence remain available for reproducibility. |
| `run_phase5_holdout_v1_validation` | One-shot official validation executable removed after the result was frozen. The report is evidence, not a supported recurring command. |
| `finalize_phase5_holdout_v1` | One-off holdout finalizer removed. |
| `audit_phase5_coefficient_reconciliation` | Incident-specific audit removed. General reconciliation must be implemented as reusable validation and automated tests. |

## Removed wrappers

The following migration wrappers were removed because they embedded a
developer's filesystem layout, chained internal phase commands and duplicated
Rust orchestration:

- `run_phase5_pipeline.sh`;
- `run_phase5_incremental.sh`;
- `run_phase5_holdout_v1_tap.sh`;
- `run_pilot_bulk_continuous.sh`.
- `run_bulk_until_shutdown.sh`;
- `package_week_milestone.sh`;
- `cleanup_production_work.sh`.

The Python GaiaXPy audit, fixture-generation, schema-emission, reconstruction
and parity scripts were removed under #61. Their supported behavior is covered
by the Rust calibration implementation and Rust parity tests.

## Historical documentation

Documents describing Phase 5/5B results may still name removed commands because
those names identify how frozen evidence was originally produced. Such commands
are historical references, not instructions for the current tree.

Do not restore them to reproduce a release. Reproduction should start from the
frozen inputs and invoke current durable commands or automated tests. When a
missing capability is genuinely required, add it under a domain-oriented name,
implement it over reusable library services, document it in the normative
registry and define its long-term audience and output contract.

## Future command policy

A new executable is accepted only when all of the following are true:

1. it produces a durable outcome independent of a development phase number;
2. its audience and owner are named;
3. inputs, outputs, maturity, resume behavior and exit codes are documented;
4. core logic is reusable library code;
5. it is registered in `tool-registry.toml`;
6. it does not duplicate an existing command, Siderust capability or test;
7. generated operational output stays outside the repository source tree.
