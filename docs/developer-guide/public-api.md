# Public API policy (crate `nsb`)

Status: Authoritative contract for the first NSB release (`0.1.0`).
Audience: Library consumers, contributors, and release maintainers.
Scope: Supported surface, classification, forward-compatibility rules, and CI enforcement.

## Recommended application path

Most callers should use only the **core stable API**:

1. Construct an [`NsbEvaluator`](../../crates/nsb/src/evaluator/core.rs) from
   [`NsbModelConfig`](../../crates/nsb/src/evaluator/types.rs) presets or builders.
2. Build a [`PointQuery`](../../crates/nsb/src/evaluator/types.rs) or
   [`ThresholdQuery`](../../crates/nsb/src/evaluator/types.rs) with constructors
   (`::new`, `with_*`), not struct literals.
3. Read [`NsbResult`](../../crates/nsb/src/evaluator/types.rs) /
   [`ThresholdQueryResult`](../../crates/nsb/src/evaluator/types.rs) and per-component
   [`NsbComponentMetadata`](../../crates/nsb/src/evaluator/metadata.rs).

Typical imports from the crate root:

| Task | Primary types |
| --- | --- |
| Point evaluation | `NsbEvaluator`, `PointQuery`, `ComponentMask`, `Observer`, `Target`, `DEG` |
| Threshold / window search | `ThresholdQuery`, `ThresholdQueryResult` |
| Model configuration | `NsbModelConfig`, `MoonlightModel`, `StarlightModel`, `SiteProfileId` |
| Site presets | `NsbModelConfig::cta_s_planning()`, `SiteProfile`, `SiteProfileId` |
| Scientific maturity | `NsbComponentMetadata`, `ComponentCalibrationStatus`, `BandDiagnostic` |
| Errors | `NsbError`, `Result` |

## API classification

Every root re-export and public nested module path is intentional. Items fall into
one of four classes:

### Core stable API

Supported for normal integrations. SemVer-breaking changes require a major
version after the first release.

Includes: evaluator types (`NsbEvaluator`, queries, results, `ComponentMask`,
`Observer`, `Target`), `NsbModelConfig` and model-selection enums,
`SiteProfile` / `SiteProfileId`, crate version constants (`NSB_VERSION`,
`MODEL_VERSION`), and the [`DEG`](../../crates/nsb/src/lib.rs) re-export used in
documented equatorial constructors.

### Advanced stable API

Supported for component-level construction, calibration experiments, and
offline tooling. Stable within a major release but not required for the default
evaluator workflow.

Includes root re-exports under airglow, moonlight, starlight, zodiacal,
`solar_activity`, and the public `components::*` module tree (`Airglow`,
`StarlightMap`, `Jones2013Spectral`, geometry/profile types, F10.7 store types,
etc.).

### Scientific metadata / provenance API

Read-mostly records describing maturity, calibration, asset identity, and
diagnostics. Fields may grow; structs are `#[non_exhaustive]` where noted.

Includes: `assets` module, `NsbComponentMetadata`, site-calibration asset types,
starlight provenance/validation records, solar-activity resolution metadata,
`BandDiagnostic`, and persisted schema version constants such as
`VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION` and `F107_STORE_SCHEMA_VERSION`.

### Implementation detail / not supported

Must not appear in the public API. These remain `pub(crate)` or private:

- Noll extinction helper functions and internal geometry integrator constants
- Bundled asset filesystem paths and internal date/storage helpers
- Unit conversions and SkyCalc-specific internal quantity aliases
- `reference`, `spectrum`, and internal `window_search` orchestration

If a needed type is missing from the supported classes above, open an issue
before depending on a newly discovered path.

## Dependency types at the boundary

NSB deliberately exposes types from **Siderust**, **qtty**, and **tempoch** at
public boundaries (`Observer`, `Target`, `Time<UTC>`, radiances, angles). Callers
may construct these directly; NSB does not wrap them solely to hide dependencies.

Re-export policy:

| Dependency symbol | Policy |
| --- | --- |
| `siderust::qtty::DEG` | Re-exported as `nsb::DEG` (documented getting-started path) |
| `Geodetic<ECEF>`, `SphericalDirection<EquatorialMeanJ2000>` | Type aliases `Observer`, `Target` |
| `Time<UTC>`, `Period<UTC>`, radiance units | Used in public query/result signatures |
| Other Siderust frames/catalog helpers | Not re-exported; import from Siderust when needed |

## Forward compatibility

### Caller-constructed structs

`PointQuery`, `ThresholdQuery`, and `NsbModelConfig` are `#[non_exhaustive]`.

- **Outside** the `nsb` crate: use `::new` and `with_*` builders (or field
  assignment on values returned from builders). Struct literals and functional
  update (`..base`) are intentionally rejected.
- **Inside** the `nsb` crate: struct literals remain valid for internal tests.

`NsbModelConfig` fields stay readable and assignable after construction so
existing builder-style CLI configuration continues to work.

### Result and metadata records

`NsbResult`, `ThresholdQueryResult`, `NsbComponent`, metadata structs, and most
status enums are `#[non_exhaustive]`. Prefer field access over exhaustive
destructuring so new diagnostics can ship in minor releases.

### Closed contracts (exhaustive enums)

Some scientific taxonomies are intentionally closed:

- `F107Kind` (serde store schema with `deny_unknown_fields`)
- `ComponentMask` (bitflags composition contract)

### `NsbError`

`NsbError` is `#[non_exhaustive]`. Match documented variants you handle and keep
a wildcard arm. Treat `Display` and `source()` as the stable diagnostic
contract; do not depend on exhaustive variant matching across minor releases.

## Public API snapshot and CI

The canonical machine-readable inventory lives at
[`crates/nsb/api/public-api.txt`](../../crates/nsb/api/public-api.txt). It is
generated with [`cargo-public-api`](https://github.com/cargo-public-api/cargo-public-api)
(simplified output: `-sss`).

[`scripts/check-nsb-public-api`](../../scripts/check-nsb-public-api) enforces:

1. **Snapshot integrity** — committed baseline must exist, be non-empty, and match
   the API generated from the current tree (fail closed on missing/malformed data).
2. **Historical SemVer gate** — when `origin/main` (merge-base for PRs) already
   contains `crates/nsb/api/public-api.txt`, `cargo public-api diff
   $BASE..HEAD` runs with `--deny=removed --deny=changed`. Updating the snapshot
   in the same PR cannot hide removals or signature changes against the merge-base.
3. **Regex guard** — retained check for deliberately removed compatibility-only
   symbols (`ALL_SUPPORTED`, `python_parity`, etc.).

### Bootstrap (no prior release)

There is no GitHub release or tag baseline yet. This PR introduces the first
committed snapshot. CI requires HEAD to match that file; the historical
`diff --deny` step is skipped until `main` contains the snapshot.

### After the first release

Once a git tag and GitHub release exist:

- Keep comparing PRs against the merge-base snapshot on `main` (primary gate).
- Optionally extend the script to compare against the latest release tag that
  ships `crates/nsb/api/public-api.txt` for release-branch workflows.

Regenerating the snapshot is only allowed together with an intentional SemVer
decision; incompatible diffs must fail CI unless the baseline itself is being
introduced on `main` for the first time.

## Updating the snapshot

```bash
# Requires nightly rustdoc (same toolchain as CI)
rustup toolchain install nightly-2026-09-02
cargo install cargo-public-api --locked --version 0.50.1

scripts/check-nsb-public-api --write
git add crates/nsb/api/public-api.txt
```

Review the diff carefully. Any removal or signature change is semver-breaking
after the baseline lands on `main`.

## Related issues

- [#125](https://github.com/VPRamon/NSB/issues/125) — canonical examples and
  expanded user documentation (out of scope for the API-freeze PR beyond
  compile-checked snippets).
