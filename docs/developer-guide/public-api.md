# Public API policy (crate `nsb`)

Status: Pre-release policy; the `nsb` public API is **not frozen yet**.
Audience: Library consumers, contributors, and release maintainers.
Scope: Intended public surface, forward-compatibility design, and the transition to an enforced API freeze.

## Current pre-freeze status

The project is still defining and correcting the first public API. Public
signatures may therefore change before the explicit freeze. In particular,
correctness fixes that replace dimensionally invalid public types with the
physical types actually represented by the data are allowed during this phase.

The classifications below describe the intended support level after the freeze
and guide review today, but they are not yet a SemVer compatibility promise.
The freeze becomes effective only when `crates/nsb/api/API_FROZEN` is committed.

## Recommended application path

Most callers should prefer the intended **core API**:

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

Every root re-export and public nested module path should be intentional. Items
fall into one of four intended classes.

### Core API

Intended for normal integrations and to become stable at the public API freeze.

Includes evaluator types (`NsbEvaluator`, queries, results, `ComponentMask`,
`Observer`, `Target`), `NsbModelConfig` and model-selection enums,
`SiteProfile` / `SiteProfileId`, crate version constants (`NSB_VERSION`,
`MODEL_VERSION`), and the [`DEG`](../../crates/nsb/src/lib.rs) re-export used in
documented equatorial constructors.

### Advanced API

Intended for component-level construction, calibration experiments, and offline
tooling. After the freeze this surface is supported within the release contract,
but it is not required for the default evaluator workflow.

Includes root re-exports under airglow, moonlight, starlight, zodiacal,
`solar_activity`, and the public `components::*` module tree (`Airglow`,
`StarlightMap`, `Jones2013Spectral`, geometry/profile types, F10.7 store types,
etc.).

### Scientific metadata / provenance API

Read-mostly records describing maturity, calibration, asset identity, and
diagnostics. Fields may grow; structs are `#[non_exhaustive]` where noted.

Includes the `assets` module, `NsbComponentMetadata`, site-calibration asset
types, starlight provenance/validation records, solar-activity resolution
metadata, `BandDiagnostic`, and persisted schema-version constants such as
`VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION` and `F107_STORE_SCHEMA_VERSION`.

The Siderust dependency provenance exports
[`SIDERUST_VERSION`](../../crates/nsb/src/lib.rs) and
[`SIDERUST_SOURCE`](../../crates/nsb/src/lib.rs) identify the package actually
resolved by the locked workspace. They must agree with `crates/nsb/Cargo.toml`
and `Cargo.lock`.

### Implementation detail / not supported

These must remain `pub(crate)` or private:

- Noll extinction helper functions and internal geometry integrator constants
- Bundled asset filesystem paths and internal date/storage helpers
- Unit conversions and SkyCalc-specific internal quantity aliases
- `reference`, internal spectral orchestration, and `window_search`

If a needed type is missing from the intended supported classes above, open an
issue before depending on a newly discovered path.

## Dependency types at the boundary

NSB deliberately exposes types from **Siderust**, **qtty**, **Optica**, and
**tempoch** at public boundaries when those types are the correct domain model.
NSB should not wrap or erase physical units merely to hide dependencies.

Re-export policy:

| Dependency symbol | Policy |
| --- | --- |
| `siderust::qtty::DEG` | Re-exported as `nsb::DEG` for documented constructors |
| `Geodetic<ECEF>`, `SphericalDirection<EquatorialMeanJ2000>` | Type aliases `Observer`, `Target` |
| `Time<UTC>`, `Period<UTC>`, radiance and spectral quantity units | May appear in public signatures |
| Other Siderust frames/catalog helpers | Import from Siderust when needed |

## Forward-compatibility design

These rules are already useful before the freeze because they reduce avoidable
future breakage.

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
destructuring so new diagnostics can be added later without unnecessary breaks.

### Closed contracts

Some scientific taxonomies are intentionally closed:

- `F107Kind` (serde store schema with `deny_unknown_fields`)
- `ComponentMask` (bitflags composition contract)

### `NsbError`

`NsbError` is `#[non_exhaustive]`. Consumers should match the variants they need
and retain a wildcard arm. Existing variant shapes should still be designed with
future compatibility in mind even though the API is not frozen yet.

### Site profile inventory

`SiteProfileId` is `#[non_exhaustive]`. Prefer
`SiteProfileId::all() -> &'static [SiteProfileId]` when enumerating built-in
profiles because the return type does not encode the profile count.

## Public API CI lifecycle

[`crates/nsb-public-api-gate`](../../crates/nsb-public-api-gate) has two modes.

### Pre-freeze mode (current)

The marker `crates/nsb/api/API_FROZEN` does not exist.

CI enforces the compatibility-only source guard, but it **does not** require
`crates/nsb/api/public-api.txt` and it **does not** reject changed or removed
public signatures. This is intentional: the first-release API is still being
corrected.

A stale pre-freeze snapshot is misleading, so `public-api.txt` is not committed
as the canonical contract in this phase.

### Freeze bootstrap

When maintainers decide the public surface is ready:

1. review all public exports and signatures;
2. add `crates/nsb/api/API_FROZEN`;
3. generate `crates/nsb/api/public-api.txt` from that same tree;
4. commit the marker and snapshot together.

The first commit containing the marker is a bootstrap: the snapshot must match
HEAD, but there is no historical frozen contract to compare against yet.

### Frozen mode

Once the selected historical base also contains `API_FROZEN`, CI enforces:

1. **Snapshot integrity** — `public-api.txt` must exist, be non-empty, and match
   the API generated from HEAD.
2. **Historical SemVer gate** — `cargo public-api diff $BASE..HEAD` runs with
   `--deny=removed --deny=changed`.
3. **Compat guard** — deliberately removed compatibility-only symbols remain
   forbidden.

Updating the snapshot cannot hide a breaking change after the freeze because the
historical diff is evaluated against a previously frozen base revision.

### How `$BASE` is chosen after freeze

| Context | Base revision |
| --- | --- |
| GitHub Actions `pull_request` | Explicit `${{ github.event.pull_request.base.sha }}` via `--base` / `NSB_PUBLIC_API_BASE` |
| GitHub Actions `push` | Explicit `${{ github.event.before }}` (commit before the push) |
| Local without `--base` | Merge-base with `origin/main` when it differs from `HEAD`, otherwise `HEAD~1` |

The gate refuses an empty `HEAD..HEAD` comparison and fails closed when an
explicit non-null historical base required by frozen mode cannot be resolved.

## Generating the freeze snapshot

```bash
rustup toolchain install nightly-2026-09-02
cargo install cargo-public-api --locked --version 0.50.1

# Add the freeze marker when the project is actually ready to freeze the API.
touch crates/nsb/api/API_FROZEN
cargo run --locked -p nsb-public-api-gate -- --write
git add crates/nsb/api/API_FROZEN crates/nsb/api/public-api.txt
```

Review the generated public surface before committing the freeze. After that
point, changed or removed signatures are governed by the frozen compatibility
policy.

## Related issues

- [#125](https://github.com/VPRamon/NSB/issues/125) — canonical examples and
  expanded user documentation.
