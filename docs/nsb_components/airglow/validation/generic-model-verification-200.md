# Phase 1 verification for #200

Scope: documentation-only scientific admission decision, based on
`d4e2672303553cf35056eb2407e4b0456ca0e3c0`, reviewed 2026-09-29.
See the [decision record](generic-model-decision-200.md).

## Command results

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS, exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS, exit 0 |
| `cargo test --workspace --all-targets --locked` | PASS, exit 0; 656 passed, 6 ignored, 0 failed |
| `cargo test --workspace --doc --locked` | PASS, exit 0; 0 doctests discovered |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | PASS, exit 0 |
| `cargo build --workspace --release --locked` | PASS, exit 0 |
| `scripts/check-public-api.sh` | PASS, exit 0; pinned cargo-public-api 0.50.1 / nightly-2026-09-02 |
| Pinned coverage collection and LCOV export | PASS, exit 0; cargo-llvm-cov 0.9.0 / nightly-2026-09-02 |
| Coverage overall gate | PASS, exit 0; workspace 82.72% (floor 80%); nsb 87.34% (floor 87%) |
| Coverage diff gate | PASS, exit 0; no changed production Rust lines (0/0), not an empirical 100% coverage claim |
| `cargo test -p nsb-data-tools --test documentation_links --locked` | PASS, exit 0; 1 test |
| `git diff --check` | PASS |
| Occurrence inventory versus `git grep` at audited HEAD | PASS: all 159 matching lines classified, no omissions or extra rows |
| Original `airglow_cont.dat` SHA-256 | PASS: matches manifest and decision record |

The API gate ran the repository's **pre-freeze forbidden-symbol check**.
`API_FROZEN` is absent by the documented #185 policy, so snapshot equality and
historical SemVer checks were skipped by the gate, not waived by this change.
There is no public API delta and no snapshot update.

## Coverage commands

The final collection used the repository-pinned tool versions and unchanged
coverage policy. The diff was generated from the actual uncommitted worktree
against audited HEAD; it is empty for Rust sources because this change only
adds/edits documentation and the audit TSV.

```bash
cargo +nightly-2026-09-02 llvm-cov clean --workspace
cargo +nightly-2026-09-02 llvm-cov --workspace --all-features --doctests --locked --no-report
cargo +nightly-2026-09-02 llvm-cov report --lcov --output-path /tmp/nsb-issue-200-checks/coverage.lcov
git diff HEAD -- '*.rs' > /tmp/nsb-issue-200-checks/working-tree.diff
scripts/coverage-gate.sh overall --lcov /tmp/nsb-issue-200-checks/coverage.lcov
scripts/coverage-gate.sh diff --lcov /tmp/nsb-issue-200-checks/coverage.lcov --diff-file /tmp/nsb-issue-200-checks/working-tree.diff
```

All commands above exited 0. The gate additionally reported nsb-cli 93.74%
and nsb-data-tools 79.70%; those have no separate floor. The collection included
all features and found zero doctests. Logs and LCOV from this session are under
`/tmp/nsb-issue-200-checks/`; these are local verification artifacts, not
redistributed scientific inputs or durable repository assets.

## Environment and retries

The initial workspace run encountered the new decision record before its linked
inventory/verification files were written; the documentation-link test failed.
The final run includes those files. No test or tolerance was weakened.

The API tool's first installation failed because OpenSSL development files were
missing. A `libssl-dev` package was downloaded and extracted under
`/tmp/nsb-issue-200-tool-deps`; no system package or repository dependency was
changed. The successful API run supplied `OPENSSL_INCLUDE_DIR`,
`OPENSSL_LIB_DIR` and `C_INCLUDE_PATH` for that staging directory.

The initially installed cargo-llvm-cov was 0.8.7; the repository-pinned 0.9.0 was
installed before the final clean collection. The first collection also failed
to link `-lpython3.14`: the runtime library existed but its development linker
name was absent. A temporary `libpython3.14.so` symlink to the installed library
was staged outside the repository. Final coverage uses `LIBRARY_PATH` pointing
to that temporary directory, with the pinned nightly and all features/doctests.
Coverage thresholds and exclusions are unchanged.

## Scientific and data verification limits

No new generic asset, generator, calibration schema, runtime path, or benchmark
was introduced. Therefore deterministic generation, malformed-new-asset tests,
calibration tests, new-path benchmarks, and independent observational residuals
are **not available**, not passing results. Software checks above validate the
existing workspace and documentation only. They cannot establish geographic
portability or satisfy #200's scientific acceptance criteria.
