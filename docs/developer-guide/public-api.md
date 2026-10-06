# Public API policy (crate `nsb`)

Status: First-release API is **frozen** for the `0.1.0` release baseline and
protected by `crates/nsb/api/API_FROZEN` plus
`crates/nsb/api/public-api.txt` on the reviewed tree.
Audience: Library consumers, contributors, and release maintainers.
Scope: Intended public surface, forward-compatibility design, and enforced
API freeze via direct `cargo-public-api` checks (`scripts/check-public-api.sh`).

## Freeze status

The API was initially frozen after the minimization work in #175. Issue #185
then temporarily returned the crate to pre-freeze mode so the generic typed
site-profile surface could replace the observatory-specific contract. That
redesign is complete and #185 is closed.

The `0.1.0` release baseline therefore restores `crates/nsb/api/API_FROZEN` and
uses the committed `public-api.txt` as the reviewed snapshot. The commit that
introduces the marker over a historical base without it uses the documented
freeze-bootstrap semantics below; after that baseline, snapshot equality and
historical removed/changed API checks are blocking.

Behavioral contracts that `cargo-public-api` cannot see (Airglow
selection/outcome, `ComponentMask::DEFAULT`/`ALL`, Starlight map ownership,
and site-profile maturity invariants) remain covered by dedicated regression
tests under `crates/nsb/tests/`.

## Recommended application path

Most callers should prefer the intended **core API**:

1. Construct an [`NsbEvaluator`](../../crates/nsb/src/evaluator/core.rs) from
   [`NsbModelConfig`](../../crates/nsb/src/evaluator/types.rs) presets or builders.
2. Build a [`PointQuery`](../../crates/nsb/src/evaluator/types.rs) or
   [`ThresholdQuery`](../../crates/nsb/src/planning/types.rs) with constructors
   (`::new`, `with_*`), not struct literals.
3. Read [`NsbResult`](../../crates/nsb/src/evaluator/types.rs) /
   [`ThresholdQueryResult`](../../crates/nsb/src/planning/types.rs) and per-component
   [`NsbComponentMetadata`](../../crates/nsb/src/evaluator/metadata.rs).

Typical imports from the crate root:

| Task | Primary types |
| --- | --- |
| Point evaluation | `NsbEvaluator`, `PointQuery`, `ComponentMask`, `Observer`, `Target`, `DEG` |
| Threshold / window search | `ThresholdQuery`, `ThresholdQueryResult`, `SiteWindowContext` |
| Model configuration | `NsbModelConfig`, `AirglowModel`, `AirglowSelection`, `MoonlightModel`, `StarlightProduct`, `ZodiacalModel`, `ZodiacalExtinction`, `SiteProfileTag`, `SiteProfile`, `GenericClearSky`, `CalibrationStatus` |
| Site profiles | `SiteProfile::<GenericClearSky>::generic_clear_sky()`, `SiteProfile::<P>::planning(...)` with caller-defined `P: SiteProfileTag`; `AtmosphericConditions` under `nsb::site` |
| Scientific maturity | `NsbComponentMetadata`, `ComponentCalibrationStatus`, `BandDiagnostic` |
| Errors | `NsbError`, `Result` |

## API classification

Every root re-export and public nested module path should be intentional. Items
fall into one of four intended classes.

### Core API

Intended for normal integrations and to become stable at the public API freeze.

Includes evaluator types (`NsbEvaluator`, queries, results, `ComponentMask`,
`Observer`, `Target`), opaque `NsbModelConfig` with getters/builders,
model-selection enums (`AirglowModel`, `AirglowSelection`, `MoonlightModel`,
`ZodiacalModel`, `ZodiacalExtinction`, `StarlightProduct`), `SiteProfileTag`,
`SiteProfile`, `GenericClearSky`, `CalibrationStatus`, crate version constants
(`NSB_VERSION`, `MODEL_VERSION`), and
the [`DEG`](../../crates/nsb/src/lib.rs) re-export used in
documented equatorial constructors. Site profile detail types also live under
`nsb::site`. Observatory-named planning presets are not part of the core API.

### Advanced API

Intended for a concrete specialized configuration or inspection need that the
evaluator cannot express through its defaults. Airglow's public scientific model
selector is root-exported as `AirglowModel`; the advanced
`components::airglow` route also exposes geometry/profile types that configure
`NsbModelConfig::with_airglow_geometry`. Direct geometry evaluation,
integrator-resolution controls, and geometry-metadata construction are internal
validation/runtime details. `Airglow`, `AirglowContinuum`, and their
component-only output are implementation details; applications evaluate Airglow
through `NsbEvaluator` results.

Airglow separates **selection policy** (`AirglowSelection::{Automatic,
Explicit}`) from **scientific model identity** (`AirglowModel`) and from
**evaluation outcome** (`AirglowEvaluationOutcome`). Defaults use `Automatic`;
until #157 admits a global climatological model, automatic policy resolves to a
temporary Paranal-derived planning fallback with typed
`AirglowFallbackReason::GlobalPlanningModelUnavailable` in
`NsbComponentMetadata::airglow_selection`. Explicit `with_airglow_model` /
`with_airglow_selection(Explicit(...))` wins and never silently switches models.
`describe_components()` reports selection metadata only and must not invent
`airglow_evaluation`. Both enums are `#[non_exhaustive]` so later validated
models and climatology can extend the contract without redesigning
`NsbModelConfig`.

The concrete continuum/evaluator remains internal. Scientific model identity is
separate from `AirglowGeometryModel` (line-of-sight/emitting-volume geometry)
and site profiles (assumptions and evidence-backed maturity via typed
`SiteProfile<P>`). Evaluated
Airglow metadata exposes selection kind, requested/resolved model, typed
fallback state, and physical outcome while asset provenance/schema/checksum,
geometry metadata, site maturity, and `MODEL_VERSION` retain their distinct
meanings. Resolved model identity lives only under `airglow_selection`
(no duplicate `airglow_model` metadata field).

Moonlight follows the same runtime-selection architecture at a smaller public
surface. `MoonlightModel` is root-exported from the Moonlight component domain,
is `#[non_exhaustive]`, and is the durable scientific selection contract.
`NsbModelConfig::default()` deterministically selects
`MoonlightModel::Jones2013Spectral`; callers can select either that
wavelength-resolved model or the deliberately supported published
`MoonlightModel::KrisciunasSchaefer1991` reference model with
`with_moonlight_model` and inspect the choice with `moonlight_model`.

The concrete Jones and Krisciunas–Schaefer evaluator types, `MoonOutputs`,
`DEFAULT_K_EXT`, model-specific range-search helpers/constants, and the Jones
extinction-scale tuning hook are implementation or validation details rather
than a second public evaluation API. Applications evaluate Moonlight through
`NsbEvaluator` and receive the shared `NsbComponent` result contract.

Moonlight scientific model identity is independent from site-profile selection.
For Jones 2013, site profiles select the atmospheric assumptions used by the
spectral model. The K&S 1991 published-reference path instead preserves its
validated fixed `k = 0.172 mag/airmass` parameterization, so selecting a site
profile does not change K&S numerics. In neither case does a profile silently
replace the selected `MoonlightModel`. The CLI model audit reports the selection
through the canonical `MoonlightModel::as_str()` identity. Adding a
component-specific Moonlight model field or a generic cross-component identity
framework is deferred to the separate metadata review.
Starlight deliberately differs from Airglow and Moonlight: the durable public
choice is a data product, not a scientific-model implementation. The
`#[non_exhaustive] StarlightProduct` enum is owned by `components::starlight`
and root-exported for normal configuration. It selects the validated bundled
Gaia DR3 XP product, an explicit caller experimental map, or a manifest-admitted
validated external map. `NsbModelConfig::with_starlight_product` configures the
selection and `starlight_product()` inspects it.

The concrete directional `Starlight` evaluator and `StarlightOutputs` are
runtime implementation details. Applications evaluate Starlight through
`NsbEvaluator` and receive the shared `NsbComponent` contract. Advanced callers
may construct or inspect `StarlightMap` / `StarlightPixel`, load
`ValidatedStarlightMap`, and retain `StarlightProvenance` /
`StarlightValidationDiagnostics`. `StarlightMap::pixel_at` is an inspection API;
the target-to-Galactic transform and component radiance evaluation remain owned
by `NsbEvaluator`.

Zodiacal follows the component-owned selector pattern while preserving a
scientifically important independent propagation dimension. The
`#[non_exhaustive] ZodiacalModel` enum is root-exported and currently contains
`ZodiacalModel::Leinert1998`, with stable identity `leinert-1998`.
`NsbModelConfig::generic_clear_sky()` selects it deterministically;
`with_zodiacal_model` and `zodiacal_model` provide the supported
configuration/inspection contract.

`ZodiacalExtinction` is not a scientific source-model identity. It independently
selects atmospheric propagation: `Noll2012Approx` is the default and
`None` disables attenuation. Callers use `with_zodiacal_extinction` and
`zodiacal_extinction`. The evaluator reports the selected source model and
propagation truthfully in provenance, and the CLI model audit exposes both
machine-readable identities.

The advanced `nsb::transport` module is the reusable atmospheric-transport
foundation. Supported capabilities are identity transport and direct
wavelength-dependent Beer–Lambert extinction, applied only through
origin-checked spectral APIs (`apply_energy_spectral` /
`apply_photon_spectral`). Zodiacal evaluation does **not** yet consume it;
legacy `ZodiacalExtinction` remains the runtime path until an explicit
migration lands. Single-scattering request/result types are intentionally
absent from the frozen surface. See
[Atmospheric transport](../specifications/atmospheric-transport.md).

The concrete `ZodiacalLight` evaluator, `ZodiacalOutputs`, the removed
wavelength-resolved `ZodiacalSpectrum` application surface, custom brightness
grid/source injection, and solar-spectrum replacement are implementation or
validation details. The first release deliberately does not freeze caller-
supplied grids or spectra because they have no product-admission, validation, or
runtime-provenance contract comparable to Starlight. Applications evaluate
Zodiacal light through `NsbEvaluator` and receive the shared `NsbComponent`
contract.

Other advanced component models and offline F10.7 store types remain available
through their deliberate component or `solar_activity` routes.

### Scientific metadata / provenance API

Read-mostly records describing maturity, calibration, asset identity, and
diagnostics. Fields may grow; structs are `#[non_exhaustive]` where noted.

Includes `data::bundled`, `NsbComponentMetadata`, site-calibration asset
types, starlight provenance/validation records, solar-activity resolution
metadata, `BandDiagnostic`, and persisted schema-version constants such as
`components::airglow::VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION` and
`F107_STORE_SCHEMA_VERSION`.

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
- Moonlight concrete evaluators, component-only outputs, tuning constants, and model-specific search helpers
- Starlight concrete evaluator/component-only output and the removed `StarlightModel` naming
- Zodiacal concrete evaluator/output, custom brightness-grid injection, and solar-spectrum replacement
- `reference` and internal spectral/threshold-search orchestration

If a needed type is missing from the intended supported classes above, open an
issue before depending on a newly discovered path.

## Dependency types at the boundary

NSB deliberately exposes types from **Siderust**, **qtty**, **Optica**, and
**tempoch** at public boundaries when those types are the correct domain model.
NSB should not wrap or erase physical units merely to hide dependencies. For
physical quantities, `nsb::units` is the canonical facade: internal NSB code and
downstream workspace crates should prefer it over importing `qtty` directly.

Re-export policy:

| Dependency symbol | Policy |
| --- | --- |
| `qtty` | Re-exported as `nsb::units::qtty`; common qtty quantity modules are also available directly under `nsb::units` |
| `siderust::qtty::DEG` | Re-exported as `nsb::DEG` for documented constructors |
| `Geodetic<ECEF>`, `SphericalDirection<EquatorialMeanJ2000>` | Type aliases `Observer`, `Target` |
| `Time<UTC>`, `Period<UTC>`, radiance and spectral quantity units | May appear in public signatures |
| Other Siderust frames/catalog helpers | Import from Siderust when needed |

## Forward-compatibility design

These rules are already useful before the freeze because they reduce avoidable
future breakage.

### Caller-constructed structs

`PointQuery`, `ThresholdQuery`, and `NsbModelConfig` are `#[non_exhaustive]`.
`StarlightPixel`, `StarlightProvenance`, and `StarlightValidationDiagnostics`
are also non-exhaustive records; construct maps/provenance through their public
constructors and builders rather than external struct literals.

- **Outside** the `nsb` crate: use `::new` and `with_*` builders (or field
  assignment on values returned from builders). Struct literals and functional
  update (`..base`) are intentionally rejected.
- **Inside** the `nsb` crate: struct literals remain valid for internal tests.

`NsbModelConfig` fields are private. Inspect choices through getters and mutate
configuration only via `with_*` builders.

### Result and metadata records

`NsbResult`, `ThresholdQueryResult`, `NsbComponent`, metadata structs, and most
status enums are `#[non_exhaustive]`. Prefer field access over exhaustive
destructuring so new diagnostics can be added later without unnecessary breaks.

### Closed contracts

Some scientific taxonomies are intentionally closed:

- `F107Kind` (serde store schema with `deny_unknown_fields`)

### `ComponentMask::DEFAULT` / `ALL` compatibility

`ComponentMask` is a bitflags composition contract. After freeze:

- `DEFAULT` is the **frozen first-release default composition** (zodiacal,
  airglow, moonlight, and starlight when a production map is bundled).
- `ALL` is an **alias of that same frozen set**, not “every component the crate
  ever implements.”
- Newly introduced physical components (for example Twilight, #159) are
  **opt-in** after freeze. Adding a new bit must not silently change scientific
  results for callers that use `DEFAULT` / `ALL` / `PointQuery::new` defaults.
- Intentional default-composition changes require an explicit model-contract /
  version change, release notes, and regression updates.

### Starlight map ownership

`StarlightProduct::{ExperimentalMap, ValidatedExternalMap}` hold
`Arc<…>` shared ownership so evaluator construction from a caller-provided map
does not deep-copy HEALPix pixel data. Prefer
`with_shared_experimental_map` / `with_shared_validated_external_map` when the
caller already owns an `Arc`.

### `NsbError`

`NsbError` is `#[non_exhaustive]`. Consumers should match the variants they need
and retain a wildcard arm.

### Site profiles

Observatory location and scientific profile are separate concerns. The core crate
exports [`SiteProfileTag`](../../crates/nsb/src/site/mod.rs),
[`SiteProfile<P>`](../../crates/nsb/src/site/mod.rs), and
[`GenericClearSky`](../../crates/nsb/src/site/mod.rs). Application crates define
zero-sized marker types implementing `SiteProfileTag`; `const NAME` is
presentation and serialization metadata only — it does not select behavior or
maturity.

Construct profiles with
`SiteProfile::<GenericClearSky>::generic_clear_sky()` or
`SiteProfile::<P>::planning(representative_altitude, atmosphere, provenance)`.
The planning constructor validates finite altitude, physical atmospheric inputs
(including pressure), and non-empty provenance, returning `Result`. Attach a
profile through `NsbModelConfig::with_site_profile`, which type-erases internally;
inspect metadata with `site_profile_name()` and component calibration getters.

Public constructors produce only `CalibrationStatus::GenericFallback` or
`PlanningPreset`. External markers cannot construct
`CalibrationStatus::Calibrated` through the public API: a profile cannot promote
its own evaluator metadata to production. A future calibrated path must connect
to the repository's evidence/admission contract. Observatory-named presets (CTAO
North/South, Python `SiteProfile`, and similar) live in application layers with
local `SiteProfileTag` implementations (for example `nsb-cli`).

Profile resolution, resolved atmosphere, and Airglow template selection remain
internal. Evaluation always uses the bundled Paranal-derived continuum; profile
metadata cannot claim a custom Airglow template.

## Public API CI lifecycle

[`scripts/check-public-api.sh`](../../scripts/check-public-api.sh) replaces the
former `nsb-public-api-gate` crate (#176) and drives pinned `cargo-public-api`
directly.

### Pre-freeze mode

When `crates/nsb/api/API_FROZEN` is absent, CI runs only the forbidden-public-API
debt guard against the generated API. Snapshot equality and historical SemVer
rejection are disabled so the first-release surface can still be corrected.

### Freeze bootstrap

When maintainers decide the public surface is ready:

1. review all public exports and signatures;
2. add `crates/nsb/api/API_FROZEN`;
3. generate `crates/nsb/api/public-api.txt` from that same tree;
4. commit the marker and snapshot together.

The first commit containing the marker is a bootstrap: when the selected
historical base lacks `API_FROZEN`, the snapshot must match HEAD and historical
`BASE..HEAD` comparison is skipped. Once HEAD is frozen, a check **without** a
usable historical base fails closed — missing base is never treated as success.

### Frozen mode

Once the selected historical base also contains `API_FROZEN`, CI enforces:

1. **Snapshot integrity** — `public-api.txt` must exist, be non-empty, and match
   the API generated from HEAD.
2. **Historical SemVer gate** — `cargo public-api diff $BASE..HEAD` runs with
   `--deny=removed --deny=changed`.
3. **Forbidden-API guard** — deliberately removed public debt remains absent.

Updating the snapshot cannot hide a breaking change after the freeze because the
historical diff is evaluated against a previously frozen base revision.

### How `$BASE` is chosen after freeze

| Context | Base revision |
| --- | --- |
| GitHub Actions `pull_request` | Explicit `${{ github.event.pull_request.base.sha }}` via `--base` / `NSB_PUBLIC_API_BASE` |
| GitHub Actions `push` | Explicit `${{ github.event.before }}` (commit before the push) |
| Local with `--base REV` | Explicit historical revision |

Invalid `BASE == HEAD` and empty historical comparisons fail closed.

## Generating the freeze snapshot

```bash
rustup toolchain install nightly-2026-09-02
cargo install cargo-public-api --locked --version 0.50.1

# Add the freeze marker when the project is actually ready to freeze the API.
touch crates/nsb/api/API_FROZEN
scripts/check-public-api.sh --write
git add crates/nsb/api/API_FROZEN crates/nsb/api/public-api.txt
```

Review the generated public surface before committing the freeze. After that
point, changed or removed signatures are governed by the frozen compatibility
policy.

## Related issues

- [#125](https://github.com/VPRamon/NSB/issues/125) — canonical examples and
  expanded user documentation.
