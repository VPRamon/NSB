# Validation record

Measured on 2026-10-12. The production engine is commit `3c14f03`; subsequent
commits add independent discrepancy regressions, benchmark comparison/reporting
and the practical default test profile. The integrated physical component
formulas and spectral integration code are unchanged.

| Gate | Result |
| --- | --- |
| `cargo test --workspace --all-targets --locked` | 883 passed, zero failures across 48 test binaries; seven explicitly ignored benchmark/tooling cases run separately where relevant. Default test profile: optimization 1, debug assertions enabled. |
| Release `planning::discovery_tests` | 20 passed, zero failures. |
| All-feature Clippy (`--workspace --lib --bins --tests --examples --locked -- -D warnings`) | Passed. |
| Workspace library/binary check and NSB no-default-feature check | Passed locally and in CI. |
| Rust 1.89 workspace check | Passed; local all-feature check also passed. |
| Formatting, license headers and cfg(test) boundaries | Passed. |
| Locked dependency/license policy | Passed. |
| Public API snapshot and historical comparison with `origin/main` | Passed: no removed or changed declared signatures. Public dependency type-identity migration is separately documented. |
| Installed final abi3 wheel, Linux Python 3.11 and 3.14 | 40 passed on each interpreter, using released Siderust/Tempoch 0.4 distributions. |
| GitHub installed-wheel jobs | Linux (3.10 and 3.14), macOS (3.11), Windows (3.10) passed. |
| CLI integrations | Nine window-contract tests passed, including authoritative checks of displayed millisecond band endpoints. |
| Publishable crate | `cargo package -p nsb --locked` verification passed; CI also checks notices, size and exclusion of unapproved scientific assets. |
| Dependency A/B separation | All 67 exported records exactly identical. |

The ordinary all-target command intentionally does not execute ignored release
benchmark matrices. Their complete usable-output evidence is recorded in
`production.jsonl` / `production.md`; process measurements are in
`cli-production.jsonl` / `cli-production.md`. Historical certificate-only
feasibility timings are not evidence for active adaptive performance.

## Scientific discrepancies resolved

| Finding | Diagnosis and permanent coverage |
| --- | --- |
| Smooth compact pulse at 7,200 seconds in a six-hour interval | Scan600 witnesses an event hidden from the original adaptive nodes. All interlaced and independent interior validation fixes this alias; symmetric and omitted-node regressions remain. |
| Approximately 53 ms Zodiacal event | Adaptive resolves a real excursion missed by scan10; an independent local scan at 1 ms confirms it. |
| 2.4006 s PALACE/Jones rejected excursion | Scan30 joins accepted time across the rejected excursion. Independent local scan at 1 ms confirms both adaptive boundaries. |
| 87.6283 s twilight rejection, CTAO-S batch target 36 | Scan120 falsely accepts time just after a physical jump. Exact boundary/interior values and a separate 10 ms scan validate adaptive's later accepted start. |
| Artificial UTC-midnight fragmentation | Flattening split physical times created a sub-microsecond gap. Local offsets preserve original endpoints; both lunar models have a permanent 30-day regression. |
| CLI min/max interval subtraction | Replaced by shared inclusive-band discovery, with accepted-side brackets for both thresholds and inward UTC/Python/CLI representations. Analytic and all-mask physical tests cover crossings, equality, tangency, jumps and narrow periods. |

No known adaptive false-positive or missed validated legacy witness remains in
the completed finite acceptance cases. This statement does not extend to an
arbitrary unvisited black-box excursion. The checked-fit / resolved-model
assumptions, explicit ambiguity errors and strict certificate distinction are
specified in the [production status](../adaptive-threshold-production-status.md).
Finer scans themselves have demonstrated false positives and missed excursions;
comparison differences are classified, not automatically attributed to adaptive
or to the Siderust upgrade. Unexplained differences abort benchmark acceptance.

## Release decision

The frozen 0.1 sampled methods and their caller-selected step retain their
behavior. Adaptive is explicit in Rust/Python and `window --adaptive`; minimum
CLI constraints always use the corrected shared band. Only obsolete CLI
interval subtraction is removed. The sampled implementation remains necessary
for the existing contract; strict certification remains necessary for its
stronger guarantee.

Switching the default and removing sampled internals requires an explicit
release-policy decision. The mandatory Tempoch 0.9 to 0.10 public type-identity
change also requires downstream dependency alignment and release review;
signature snapshots cannot establish cross-version type identity. The separate
CTAO service migration is documented but its implementation is unchanged.
