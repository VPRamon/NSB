# PR #275 release-readiness audit

Audit date: 2026-10-23. Baseline: published `v0.1.1`; development source before
housekeeping: `4c2c3bcd61333dd7f2b74211b67eac6752a273ba`.
Independent evidence repository reviewed read-only at
`VPRamon/nsb-validation@3191bfc7597b5a2add95456aeb2ea1b906979d22`.

## Decision and unresolved release gate

The direct Siderust 0.15.0 migration failed the all-feature workspace check with
incompatible Tempoch/coordinate types. Registry siderust-py 0.4.0 still uses
Siderust 0.14.1. Restoring the previous immutable graph preserves one upstream
identity and truthful CLI/Python provenance, but does **not** resolve #265.
The temporary negative packaging helper, explicit Git-source policy exception,
fixture pin and tag release guard remain necessary. Ordinary Cargo packaging
still fails because registry 0.14.1 lacks the required optics APIs. Do not publish
this tree or close #265 on the strength of a patched checkout/wheel build.

## Scope and retained records

| Material | Classification and action |
|---|---|
| Registered compact map, sidecar, registry, PALACE/solar assets | Current runtime contracts; preserve bytes and paths |
| Scientific asset manifests, notices and checksum references | Reproducibility and redistribution evidence; preserve |
| Starlight release-candidate decisions and approval-history | Required legal/historical evidence; preserve |
| `docs/validation/`, component experiment reports and reference fixtures | Historical or numerical evidence; preserve evaluated identities |
| Independent numerical reference generators | Reproducibility tools; retain |
| Legacy integrated maps, `PreScatteredAtmosphere`, explicit Airglow and opt-in transport | Supported input/physical/experimental contracts; retain |
| Exact pre-v0.1.0 snapshot-hash exception | Obsolete authorization; remove, with regular SemVer failure regression retained |
| Stale current guidance on sampling, defaults and #103 | Correct prose without editing historical experimental bytes |

No scientific evidence files were deleted, regenerated or moved. The frozen
release-candidate README and licensing ATTRIBUTION are themselves checksum-pinned;
their historical pending-approval prose remains unchanged. Current status is
explained in the validation matrix and user guide rather than invalidating the
approved bundle. No speculative
public API removal was made. See the changelog for all eleven merged scientific
PRs since v0.1.1; the new methods, records, transport settings and calibration
contracts are additive. The actual SemVer gate remains mandatory; textual diff
ordering alone does not establish compatibility. Python stubs already declare
restricted-band component results and calibration selection, and are unchanged.

## Scientific admission

[Validation status](../specifications/validation.md) is the canonical summary;
[consumer requirements](../specifications/validation-consumer.md) define new
campaign identity. The historical Patat result and its negative findings remain
pinned to `1b2556727205694c7765ef0d254c5d1b301582ec`. Oxygen diagnostics on reused
data are development evidence. DESI and LVM remain blocked for absolute accuracy.
No new independent observations were executed or claimed by this PR.

## Release workflow inspection

`publish-github-release` already depends on `record-publication` with
`!cancelled() && needs.record-publication.result == 'success'`. That condition
handles the skipped recovery branch which suppressed the old release path.
The job requires both wheels and sdist, checks for an existing release and uses
`--verify-tag`. This audit did not run a publication workflow or create a tag.

## Executed verification

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets --all-features --locked`: passed on the
  restored graph; failed on the proposed 0.15.0 graph (type identity errors).
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`:
  passed.
- `cargo +1.89 check --workspace --all-targets --all-features --locked`: passed.
- `scripts/check-public-api.sh --base v0.1.1`: frozen snapshot and real historical
  SemVer gate passed, run in a disposable clone to isolate checkout mutations.
- Installed repository wheel: 140 Python tests passed, 11 opt-in cases skipped.
- PEP 517 isolated sdist wheel build and installation: passed; its installed wheel
  also passed 140 tests, with 11 opt-in cases skipped.
- `NSB_INTERFACE_TESTS=1 ... pytest python/tests/test_interface_equivalence.py -q`:
  all 11 opt-in Rust/Python/CLI equivalence cases passed using installed extension.
- Scientific asset registry checksum test and Siderust provenance test: passed.
- API lifecycle, coverage-gate and cfg-test-boundary script regressions: passed.
- License headers, release metadata and changed-document local links: passed.
- Cargo archive contents include all three notices/licenses and approved compact
  runtime pair, excluding the four repository-only candidates. Sdist PEP 639
  license check passed.
- Ordinary `cargo package -p nsb --locked` and publish dry-run: failed isolated
  registry compilation on missing optics APIs; these are release blockers.
- All-feature doctests: initial host interpreter linkage failed (`-lpython3.14`
  unavailable); retry with `PYO3_PYTHON=$PWD/.venv/bin/python` (3.12) passed.
- Initial workspace tests caught checksum drift from editing two frozen approval
  documents; those exact files were restored rather than changing pinned hashes.

Full instrumented coverage, hosted multi-platform CI and live release publication
were not executed locally. Script coverage policy regression tests do not prove
full-workspace coverage. The release workflow received static inspection only.
No package, release tag or GitHub Release was published; no issue was closed.

Final rerun: `cargo test --workspace --all-targets --locked` passed after restoring
both frozen approval documents. `cargo deny check` passed advisories, bans,
licenses and source policy (existing unmatched-ignore warnings remain). The
retained `verify-single-scattering-package.sh` passed its precise negative
registry check and compiled the extracted archive using the explicit immutable
pin; its success is deliberately **not** registry publication readiness.
