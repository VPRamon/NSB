# Coverage policy

Status: Current CI and maintainer coverage contract.
Audience: Contributors and release maintainers.
Scope: How NSB measures coverage, which floors are blocking, and when
thresholds may change.
Non-goals: This page does not chase 100% coverage or replace scientific
validation.

Numeric floors, measured baselines, and exclusion lists live in the repository
root [`coverage-policy.toml`](../../coverage-policy.toml). That file is the
only CI-consumed source of thresholds. Do not copy percentages into workflow
YAML or duplicate them here.

## Why coverage is gated

CI already collected workspace coverage, published a summary, and uploaded an
HTML report. Those reports remain. In addition, ready-for-review pull requests
and pushes to `main` fail when:

1. workspace line coverage falls below the recorded floor;
2. `nsb` (runtime/core) line coverage falls below the recorded floor;
3. on pull requests, executable changed lines in production Rust sources fall
   below the diff-coverage floor.

Coverage is a regression signal. It is not a substitute for contract tests,
scientific validation, or review.

## How to run coverage locally

Reproduce the CI collection:

```bash
rustup toolchain install nightly --component llvm-tools-preview
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --workspace --all-features --doctests --locked --no-report
cargo +nightly llvm-cov report --cobertura --output-path coverage.xml
cargo +nightly llvm-cov report --json --output-path coverage.json
cargo +nightly llvm-cov report --html --output-dir coverage_html
```

Enforce the same gates CI uses, without collecting coverage again:

```bash
cargo run --locked -p nsb-coverage-gate -- overall --report coverage.json
cargo run --locked -p nsb-coverage-gate -- diff --report coverage.json --base origin/main
```

The HTML report is written under `coverage_html/`. Open
`coverage_html/html/index.html` (cargo-llvm-cov may use `coverage_html/index.html`
depending on version).

`nsb-coverage-gate` is an in-repository Rust checker. It reads llvm-cov JSON and
`git diff -U0` against the merge base. Coverage data is not uploaded to a
third-party hosted service.

## What the floors mean

| Gate | When | Source | Blocking? |
| --- | --- | --- | --- |
| Workspace line floor | PRs and `main` | llvm-cov workspace totals | Yes |
| `nsb` line floor | PRs and `main` | files under `crates/nsb/` in the same report | Yes |
| Diff production lines | Pull requests | changed executable lines in production `src/` | Yes |
| Function/region | Always printed | same report | No (diagnostic) |
| `nsb-cli` / `nsb-data-tools` lines | Always printed | same report | No |

`nsb-cli` and `nsb-data-tools` are recorded but not given separate floors.
Their production changes are still subject to the diff gate. Broad CLI and
data-tool coverage is expected to move after the obsolete-code cleanup
([#122](https://github.com/VPRamon/NSB/issues/122)) and test-suite audit
([#123](https://github.com/VPRamon/NSB/issues/123)).

The current `baseline_kind` in `coverage-policy.toml` is
`provisional-pre-audit` until those issues land. Re-measure on the post-audit
tree and replace the recorded SHA, toolchain versions, observed percentages,
and floors together.

## Diff-coverage semantics

The diff gate:

- diffs `*.rs` from `git merge-base <base> HEAD` to `HEAD` (`<base>` is the PR
  base SHA in GitHub Actions, otherwise `origin/main`);
- treats files under `crates/<crate>/src/` as production code;
- ignores integration tests (`crates/*/tests/`), unit-test modules named
  `tests.rs`, benches, and examples as coverage *targets*;
- ignores non-executable changed lines (comments, blank lines, and other
  spans llvm-cov did not instrument);
- fails if a changed production file is absent from the coverage report
  entirely;
- lists uncovered changed production lines and missing files in the job log.

The target is approximately 90% of executable changed production lines. The
exact floor is `diff.changed_production_lines` in the policy file.

## Exclusion policy

Exclusions must be:

- listed in `coverage-policy.toml`;
- narrow (not a crate or large production module);
- technically justified in the policy notes.

Forbidden: excluding code to manufacture a passing percentage; marking normal
production branches `coverage(off)`; dummy tests whose only purpose is to
execute lines; deleting assertions to make coverage easier.

There are currently **no** exclusions.

## Changing thresholds

1. Reproduce the CI coverage command on the intended base.
2. Record nightly, `cargo-llvm-cov`, commit SHA, date, and observed line,
   function, and region coverage for the workspace and each crate.
3. Set floors a small margin below the reproduced line coverage so immaterial
   toolchain jitter does not fail CI.
4. Change `coverage-policy.toml` in the same commit as the measurement.
5. Do **not** lower a floor merely to make a pull request pass. If coverage
   dropped, add or restore meaningful tests, or justify a real deletion of
   covered code.

After #122 and #123, replace this provisional baseline with the post-audit
release baseline rather than keeping a pre-cleanup number.
