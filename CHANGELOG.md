# Changelog

All notable changes are recorded here. The project follows semantic versioning
once a stable public release is cut.

## 0.1.0 - 2026-09-30

### Added

- Introduced `nsb::transport`, a minimal frozen atmospheric-transport foundation
  for wavelength-resolved radiance (#187 / #193): identity and direct
  Beer–Lambert transmission over Siderust Rayleigh/Mie/ozone/airmass primitives;
  mandatory `RadianceOrigin` checks on spectral apply; model-only metadata
  without caller-invented atmosphere provenance; validated direct-path geometry.
  Unfinished scattering scaffolding and generic monochromatic quantity scaling
  are intentionally not part of the public API. Existing Zodiacal, Airglow,
  Moonlight, and Starlight runtime paths are unchanged.

### Changed

- Validated NSB against the pre-release Siderust 0.12 / PR #97 branch, aligned
  direct `tempoch` dependencies on 0.7, and consumed `qtty` and `optica` API
  types through Siderust's public re-exports. This temporary branch dependency
  must become the crates.io `siderust = "0.12"` release after publication.
- Redesigned site profiles around typed compile-time identity (#185). The public
  API exports `SiteProfileTag`, opaque `SiteProfile<P>`, and `GenericClearSky`.
  External crates and application layers define zero-sized marker types; `NAME`
  is presentation/serialization metadata only. `SiteProfile::<GenericClearSky>::generic_clear_sky()`
  and `SiteProfile::<P>::planning(representative_altitude, atmosphere, provenance)`
  are the supported constructors; both fail closed on nonphysical atmospheric
  inputs. `NsbModelConfig::with_site_profile` accepts a typed profile and
  erases it internally; `site_profile_name()` exposes metadata only. Removed
  from the public surface: `SiteProfileId`, `SiteProfileSpec`,
  `RepresentativeAltitude`, `AtmosphereSource`, public `AirglowSiteCalibration`,
  `with_airglow_calibration`, and public profile `resolve()`. CTAO North/South
  markers and CLI presets live in `nsb-cli` / Python bindings, not `crates/nsb`.
  Airglow always uses the bundled Paranal-derived continuum template; callers
  cannot claim a custom template through profile metadata. The first-release
  API freeze marker was removed so this breaking redesign can land; re-freeze
  after review.
- Replaced the `nsb-coverage-gate` Rust crate with `scripts/coverage-gate.sh`,
  keeping the same blocking overall and PR diff line-coverage floors from
  `coverage-policy.toml` without a workspace package or extra compile step.
- Refactored CLI observatory handling onto Siderust `ObservatoryCatalog`, with
  NSB bundled `[[observatory]]` extensions for CTAO-N/S, H.E.S.S., MAGIC, FACT,
  VERITAS, FAST, and GTC; separated `--site` location selection from
  `--site-profile` scientific assumptions; and documented catalog precedence
  for Siderust builtins, NSB extensions, and `--observatory-catalog` (#140).
- Finalized the post-#122/#123 release coverage baseline (`baseline_kind =
  release-post-audit`) and raised the blocking `nsb` line floor to match the
  measured tree; coverage policy and developer docs supersede the provisional
  #127 baseline (#124).
- Consolidated Airglow historical audit documents into the canonical runtime
  guide and removed obsolete development-history prose (#122).
- Removed unused `approx` dev-dependencies, orphan provisional validation
  envelope, and unused Starlight diagnostic helpers (#122).
- Prepared the `nsb` first-release public API for an eventual freeze: classified
  supported surfaces, hid implementation-only constants/helpers, added
  constructors for `PointQuery`/`ThresholdQuery`, and marked evolvable
  structs/enums/`NsbError` `#[non_exhaustive]`. The API remains pre-freeze;
  snapshot equality and historical SemVer enforcement activate only after the
  explicit `crates/nsb/api/API_FROZEN` marker is committed (#121).
- Corrected public Siderust provenance (`SIDERUST_VERSION` /
  `SIDERUST_SOURCE`) and CLI metadata to the locked crates.io package
  `0.11.1` (`crates.io:siderust:0.11.1`), with a contract test preventing
  dependency/provenance drift (#121).
- Merged Starlight build-time checksum verification so library/rustdoc builds
  no longer pay siderust const-eval SHA-256 on the bundled map (#129).

- Replaced single-pixel Starlight merge evidence with exact mergeable numeric
  accumulators, complete pixel/accounting comparison, and versioned dataset-wide
  deterministic digests (#73).
- Consolidated `nsb-data-tools` from 36 compiled binaries to 19 durable,
  capability-oriented Rust commands; removed Phase 5/5B one-shot executables,
  shell orchestration, and Python data-product programs; added pure-Rust Gaia XP
  continuous reconstruction, a normative tool registry, and CI-enforced
  documentation and maturity contracts (#58, #61).
- Consolidated `nsb-data-tools` from 36 compiled binaries to 18 durable,
  capability-oriented commands; removed Phase 5/5B one-shot executables, shell
  orchestration and the deprecated Python pilot wrapper; added a normative tool
  registry and CI-enforced documentation/maturity contracts (#58).
- Made library `ALL`, library `DEFAULT`, and CLI `all` the same production-safe
  set, with starlight included only when a validated bundled production asset is
  embedded at build time.
- Renamed the bundled starlight path as an experimental seed and made access
  explicitly opt-in.
- Cached parsed starlight and shared airglow calibration state inside
  `NsbEvaluator`.
- Standardized Siderust metadata on crates.io `siderust = 0.11.0` and the
  public source identity `crates.io:siderust:0.11.0`.
- Switched Gaia canonical starlight sources to explicit ICRS radian columns and
  added production/candidate modes to `pack_starlight_asset`.
- Expanded JSON and CSV with version, model, maturity, provenance, uncertainty,
  band-diagnostic, and asset-checksum metadata.
- Fixed magnitude cuts so the generated map, conservation sums, and
  `sources_used` diagnostics consume exactly the same filtered catalogue rows.

### Added

- Versioned, fail-closed CTAO site-calibration asset schema with strict TOML
  parsing, physical-range checks, immutable reference provenance, and promotion
  guidance while existing CTAO profiles remain planning presets (#80).
- Starlight production foundation (PR #56): normative 300–650 nm contract,
  deterministic Gaia sampling, XP continuous acquisition/reconstruction tooling,
  dual overlap/absolute uncertainty contract, frozen Phase 5 policy v1,
  independent holdout validation, fail-closed approval and candidate
  infrastructure, validation/packing/runtime foundations. The global integrated
  starlight product remains pending (#103).
- NSB-side Gaia DR3 starlight release pipeline harness: documented Gaia
  extraction recipe, Gaia XP passband source preparation, Gaia photon-flux
  HEALPix map generation path, validation report command, and candidate asset
  packer. The real bundled production asset remains pending real Gaia extraction
  and independent validation.
- Build-script plumbing for the Gaia DR3 bundled production starlight CSV/TOML:
  exactly one registered production release pair is checksum-embedded and loaded
  through the runtime validated-map contract; absent assets fail closed.
- Versioned scientific asset manifest and checksum/header verifier.
- Independent published KS91 validation fixture with units and tolerance.
- Point/component/window benchmarks and scheduled/manual benchmark workflow.
