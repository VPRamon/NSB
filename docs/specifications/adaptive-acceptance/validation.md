# Validation record

Recorded on 2026-10-12/13 (UTC); later checks are 2026-10-13 in Europe/Andorra. The successful discovery algorithm was established in commit `3c14f03`; subsequent
commits add deterministic error selection, discrepancy regressions, benchmark
comparison/reporting, test-layout cleanup, the practical default test profile and the authorized 0.1 default migration. The integrated physical component
formulas and spectral integration code are unchanged.

| Gate | Result |
| --- | --- |
| `cargo test --workspace --all-targets --locked` | 885 passed, zero failures across 48 test binaries; seven explicitly ignored benchmark/tooling cases run separately where relevant. Default test profile: optimization 1, debug assertions enabled. |
| Release `planning::discovery_tests` | 22 passed, zero failures; the separate 64-case boundary export also passed. |
| All-feature Clippy (`--workspace --lib --bins --tests --examples --locked -- -D warnings`) | Passed. |
| Workspace library/binary check and NSB no-default-feature check | Passed locally and in CI. |
| Rust 1.89 workspace check | Passed; local all-feature check also passed. |
| Formatting, license headers and cfg(test) boundaries | Passed. |
| Locked dependency/license policy | Passed. |
| Public API snapshot and historical comparison with `origin/main` | Passed: no removed or changed declared signatures. Public dependency type-identity migration is separately documented. |
| Installed final abi3 wheel, Linux Python 3.11 and 3.14 | 40 passed on each interpreter, using released Siderust/Tempoch 0.4 distributions. |
| GitHub installed-wheel jobs | Linux (3.10 and 3.14), macOS (3.11), Windows (3.10). Final committed-head results are authoritative in [PR #235 checks](https://github.com/VPRamon/NSB/pull/235/checks). |
| CLI integrations | Nine window-contract tests passed, including authoritative checks of displayed millisecond band endpoints. |
| Production implementation coverage | Local overall 88.27%, NSB 92.71%, changed production lines 93.86% (779/830); existing floors unchanged. Current-head CI repeats these gates. |
| CLI default equivalence | All six normal CLI outputs exactly reproduce the pre-migration explicit-adaptive CSV digests; see `default-dispatch.jsonl`. |
| Publishable crate | `cargo package -p nsb --locked` verification passed (143 files, about 370 KiB compressed); CI also checks notices, size and exclusion of unapproved scientific assets. |
| Dependency A/B separation | All 67 exported records exactly identical. |

The ordinary all-target command intentionally does not execute ignored release
benchmark matrices. Their complete usable-output evidence is recorded in
`production.jsonl` / `production.md`; [performance analysis](performance-analysis.md)
explains the measured tradeoff. Process measurements are in
`cli-production.jsonl` / `cli-production.md`. Historical certificate-only
feasibility timings are not evidence for active adaptive performance.

## Scientific discrepancies resolved

| Finding | Diagnosis and permanent coverage |
| --- | --- |
| Smooth compact pulse at 7,200 seconds in a six-hour interval | Scan600 witnesses an event hidden from the original adaptive nodes. All interlaced and independent interior validation fixes this alias; symmetric and omitted-node regressions remain. |
| Approximately 53 ms Zodiacal event | Adaptive resolves a real excursion missed by scan10; an independent local scan at 1 ms confirms it. |
| 2.4006 s PALACE/Jones rejected excursion | Scan30 joins accepted time across the rejected excursion. Independent local scan at 1 ms confirms both adaptive boundaries. |
| 87.6283 s twilight rejection, CTAO-S batch target 36 | Scan120 falsely accepts time just after a physical jump. Exact boundary/interior values and a separate 10 ms scan validate adaptive's later accepted start. |
| Two sub-second twilight intervals in the representative June Zodiacal/Airglow oracle | Both are accepted singleton physical endpoints expanded by midpoint refinement; five independent interior radiance probes reject their positive duration. Valid periods retain the unchanged comparison allowance. |
| Annual scan120 misses accepted islands | Independent 1 ms references confirm roughly 7.199 s (South K&S), 9.843 s (North K&S), and 13.788 s (North Jones) islands. The permanent regression preserves the annual grid phase and checks both accepted-side boundaries. [Values](accepted-islands.json). |
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

All **204** complete usable matrix scenarios pass: **105** independently confirmed
rejected-excursion reference misses, **13** accepted-island reference misses,
zero unexplained discrepancies/exhaustions. [Raw discrepancy records](confirmed-discrepancies.jsonl)
count scenario occurrences rather than unique astronomical events. Annual
100-target medians of **450–551 seconds** and paired scan600 ratios **2.39–2.50**
are accepted for offline annual planning; this is an accuracy/cost tradeoff,
not a blanket speedup. See [the compact report](performance-summary.md).

The maintainer explicitly reopened the behavior freeze on 2026-10-13 and
requested the migration in 0.1. Normal Rust/Python methods and CLI `window` now
use adaptive discovery. Public step fields/builders/arguments remain as ignored
compatibility inputs; explicit adaptive names remain aliases. The scanner and
old certificate-subdivision implementation are test-only references; one
production engine implements empirical adaptive and strict enclosure modes.

The Tempoch 0.9 to 0.10 public type migration requires downstream dependency
alignment and recompilation. Snapshot and historical API checks remain active;
these checks cannot establish cross-version public dependency type identity.
The CTAO service implementation is unchanged.
