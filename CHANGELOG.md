# Changelog

Release availability on crates.io and PyPI is confirmed only after the
tag-triggered publication workflow succeeds.

## [Unreleased]

## [0.2.0]

Release preparation; not yet published. Registry dependency compatibility is
restored; publication still awaits maintainer approval. Require siderust-py and
Python siderust >=0.5.1,<0.6, excluding the unsafe 0.5.0 query dispatcher
([upstream #13](https://github.com/Siderust/siderust-py/issues/13)). Evaluator physical defaults,
scientific assets and historical validation evidence are unchanged by the
dependency and release-metadata updates. New opt-in capabilities below do not
establish independent observational accuracy.

### Added

- Component-separated restricted-band results and single-pass evaluation in Rust
  and Python, with explicit unsupported contributions and uncertainty semantics
  (#271).
- Registered Airglow calibration workflow and opt-in experimental oxygen emission
  model; development diagnostics do not establish independent accuracy (#272).
- Opt-in multiple-scattering transport, native spectral HEALPix boundary handling
  and scalable numerical verification (#269, #270).
- Scientific publication-candidate numerical/interface evidence and reproducible
  artifact checks (#274).

- Opt-in experimental native-bin Starlight single-scattering transport with
  explicit flat photon density, site/TT/UT1 geometry, bounded wavelength
  refinement, conservative HEALPix kernel subdivision and independent WGS84/MSL
  origins, with component-separated provenance; evaluator defaults remain
  unchanged (#268).

- Experimental opt-in `transport::single` scalar plane-parallel reference with
  separate direct/single contributions, explicitly uncomputed higher orders,
  deterministic convergence checks and independent analytical/SciPy validation.
  Defaults and existing component calculations remain unchanged.

- Source-level Gaia XP interval-report example using checksum-pinned inputs,
  canonical parsing and covariance-aware integration, with deterministic
  numeric source ordering (#258).

- One validated `StarlightMap` Rust/Python API for versioned spectral products
  with configurable bins, explicit capabilities, checksum-verified loading and
  optional statistical uncertainty channels (#257).

- Typed Rust and Python restricted-band integration for supported component
  spectra within 300–650 nm, including interpolated interval boundaries and
  explicit errors for unsupported spectral contributions (#254).

### Changed

- Simplify Airglow spherical line-of-sight geometry while preserving the physical
  model and regression contract (#261).
- Consolidate external observational status and validator consumer requirements;
  preserve historical evaluated revisions and negative findings (#275).
- Replace the temporary Siderust Git graph with compatible published providers;
  preserve canonical Python objects, UTC datetime/period handling and GIL release.
- Require successful full main-branch CI for the exact release commit before
  tag publication, retaining environment, Trusted Publishing and recovery guards.
- Remove the historical snapshot-hash SemVer exemption. Every frozen comparison
  now uses the regular removal/change gate (#275).

- Reject unsupported NESTED spectral-map lookup and ground-level evaluation of
  top-of-atmosphere spectral products explicitly. Preserve the frozen Gaia XP
  map, legacy integrated products and existing production assets (#257).

- Reuse the upstream Optica clipped trapezoidal integration kernel for Airglow
  band integration, preserving existing physical models and full-band results
  (#254).

### Breaking Changes

- Rust callers constructing upstream domain types must migrate their own
  Siderust/Tempoch dependencies to the coordinated 0.15.1/0.11.1 graph; older
  provider identities cannot be mixed with NSB 0.2.0. NSB method/class names and
  evaluator defaults are preserved. Python installations require the 0.5 runtime
  distributions while retaining the canonical Observer/Direction classes.

### Dependencies

- Update MD5 0.10.6 → 0.11.0 and TOML editing 0.23.10 → 0.25.17; share
  Digest 0.11 with SHA-256 and the TOML 1.1 parser stack. Update ZIP 9.0.4 →
  9.0.5 and refresh compatible transitive dependencies.
- Remove the unused ttf-parser advisory exception; that crate is absent from
  the dependency graph. Retain the upstream paste maintenance exception.
- Migrate Siderust 0.14.1 Git → registry 0.15.1, Tempoch/core 0.10 → 0.11.1,
  Python bridges/runtime packages 0.4 → 0.5, and PyO3 0.29.3 → 0.30.1, with
  Affn 0.12.1, Principia 0.7 and Optica 0.5. Remove the Git-source exception and
  negative packaging workaround; retain Rust 1.89 and Python 3.10 minima.
  See [dependency compatibility](docs/specifications/siderust-compatibility.md).

### Fixed

- Verify the separately registered experimental oxygen asset, its checksum
  manifest and Airglow notices in release archives, alongside the main registry.

- Correct the private S10 photon-energy numerator to exact SI `h*c`. The prior
  constant differed by about 1.98e-14 relative; only S10 diagnostics change at
  that scale. Integrated photon radiances, model defaults and observational
  evidence are unchanged. An independent SI regression covers 16 cases.


- Verify and process the same manifest, GaiaXPy design and source CSV bytes in
  the source-interval utility, preventing input replacement from invalidating
  recorded scientific checksums (#258).

- Publish source-interval reports through staged, synced files without replacing
  existing output, preventing concurrent writers from overwriting a report or
  exposing partial contents (#258).

- Preserve single-bin Starlight statistical uncertainty in native-band queries
  and pixel adapters; reject contradictory per-bin/native sigmas and violations
  of necessary covariance bounds without assuming independent spectral bins
  (#257).

- Reject incomplete Starlight pixel uncertainty combinations during map
  construction instead of silently discarding supplied systematic/total values
  when statistical uncertainty is absent (#257).

- Represent missing Starlight B/V photometry as unavailable instead of physical
  zero; retain supplied full-band statistical uncertainty without inventing
  systematic uncertainty or cross-bin covariance (#257).
- Refresh compiled scientific provenance after relevant workspace source, data,
  Cargo-input and Git-state changes, including incremental rebuilds (#257).
- Reject empty shard edges, source-count overflows and inconsistent source
  accounting before scientific-product publication (#257).

- Release the Python GIL during restricted-band evaluation (#254).

### Removed

- Unused private site planning-name formatter and its self-tests; exclude the
  standalone calibration parser from production builds and remove the module's
  blanket dead-code suppression. Runtime calibration validation is retained.
- Development-transition prose, obsolete API exception documentation and the
  task-specific release audit file. API lifecycle fixtures now use synthetic
  snapshots rather than depending on a pre-release Git commit. Scientific
  evidence, approval records and numerical reference results are preserved.

- Historical exact-snapshot authorization for removing pre-v0.1.0 sampling APIs.
  The sampling methods were already absent in published v0.1.1; this removes no
  published API. Frozen snapshot equality and historical SemVer checks remain.

### Interface notes

- Rust/Python add `NsbEvaluator.integrate_band_components`,
  `RestrictedBandResult` and `RestrictedBandComponent`; retain scalar
  `integrate_band`. Unsupported spectra raise errors rather than returning zero.
- Rust adds `site::airglow_profile` calibration definition, observation, request
  and profile records; configuration accepts an explicit fitted profile. Python
  exposes calibration through TOML configuration. CLI gains the
  `nsb-airglow-calibrate` workflow. Opt-in oxygen is separate from the default
  PALACE continuum model.
- Experimental transport adds solver/convergence settings and result records for
  single, multiple, projected and native-bin Starlight evaluation. These are
  explicit library APIs; ordinary CLI/evaluator defaults do not enable them.
- Generic spectral `StarlightMap` loading, bin/support/provenance queries and
  optional uncertainty channels preserve legacy integrated-map input. Missing
  uncertainty or photometry remains unavailable; no covariance is invented.

## [0.1.1] 15/10/2026

### Changed

- Prepare the next coordinated `nsb` / `nsb-py` release by synchronizing
  the Rust workspace versions, lockfile and Python distribution metadata.
- Expose crates.io and PyPI publication as GitHub environment deployments with
  links to their package pages, including the PyPI sdist recovery job.
- Make the CI crate-size check independent of the release version.
- Preserve the frozen Rust API and scientific runtime behavior from `0.1.0`.

## [0.1.0] 15/10/2026

### Changed

- Activate the conditionally approved, lossless compact Starlight map for offline
  four-component Rust/Python defaults. Preserve source-specific scientific data
  terms, notices and Gaia noncommercial restrictions separately from AGPL code.

- Require released Siderust 0.14.1 throughout the workspace and use its typed
  lunar spectral irradiance API, including the inverse-square source correction.
- Keep optical depths, airmasses, phase densities, extinction coefficients,
  zodiacal wavelengths and fallback radiances typed through the corrected kernels.
- Correct first-order Jones scattering and local Rayleigh pressure handling,
  and the Noll zodiacal brightness units, optical-depth sign and species terms.
- Migrate planning inputs using independent millisecond references and add a
  precise test scanner that does not expand isolated endpoints into intervals.

- Make adaptive discovery the normal Rust/Python/CLI threshold search in the
  first release. Remove ignored Rust sampling fields/builders, Python sampling
  parameters and CLI step/adaptive flags; explicit adaptive methods remain
  available. Move the scanner and prior certificate-subdivision
  implementation to independent test-only references.

- Upgrade to registry Siderust 0.14.1, Tempoch 0.10, Principia 0.6 and Python
  interoperability/runtime packages 0.4; retain Rust 1.89.
- Fix adaptive low-fit aliasing with all interlaced and independent interior
  checks; retain exact physical refinement and strict certificate semantics.
- Add inclusive joint min/max adaptive bands in Rust and Python. CLI minimum
  constraints use this engine, replacing unsafe interval subtraction.
- Bound adaptive work across the whole query using deterministic Rayon batches.
- Preserve split temporal endpoints when joining fitted and certified regimes,
  fixing artificial UTC-midnight fragmentation.
- Add independently resolved Zodiacal, PALACE/Moonlight and twilight discovery
  regressions, complete schedule benchmarks and compatible installed-wheel tests.
- Refresh Paranal Zodiacal and Airglow numerical baselines for the upgraded
  time/geometry dependencies, retaining existing regression tolerances.

### Added

- Typed Rust `nsb` runtime for point night-sky-background evaluation and
  threshold-window planning over the 300–650 nm integrated photon-radiance
  contract.
- `nsb-py` Python distribution (Python 3.10+, import name `nsb`) using
  PyO3 `abi3-py310` wheels and canonical Siderust observer/direction types.
- `nsb-cli` application with point/window evaluation, observatory lookup,
  table/JSON/CSV output, configuration, and structured logging.
- Zodiacal light, Airglow, Jones 2013 and KS91 scattered Moonlight, and
  map-backed integrated-Starlight component contracts with provenance,
  maturity, validation-domain, uncertainty, and diagnostic metadata.
- Generic typed site profiles through `SiteProfileTag` and `SiteProfile<P>`.
- `nsb::transport` foundation for origin-checked spectral identity and direct
  Beer–Lambert transmission over Siderust atmospheric primitives.
- `nsb-data` maintainer tooling for reproducible scientific-asset acquisition,
  transformation, validation, reconciliation, checksums, provenance, and
  packaging.
- Deterministic PALACE v1.0 CC-BY-4.0 Airglow continuum product preserving
  separate HO2, FeO-like, unresolved O2, month, local-time, solar-activity, and
  residual-variability semantics.
- Scientifically reviewed Starlight candidate workflow with checksum-pinned
  technical validation and fail-closed production admission. The exact
  conditionally approved compact map and provenance sidecar are bundled for
  offline four-component defaults, subject to source-specific data terms.
  Repository-only candidate CSVs and reports are excluded from packages.
- Versioned scientific-asset manifests, checksum/header verification,
  independent scientific fixtures, end-to-end tests, coverage policy,
  manual benchmarks, and public API snapshot tooling.
- Tag-driven release workflow for `vMAJOR.MINOR.PATCH` that validates package
  metadata, builds Python wheels and an sdist, publishes `nsb` to crates.io,
  and publishes `nsb-py` to PyPI using Trusted Publishing/OIDC.

### Distribution

- Version `0.1.0` is shared by the publishable `nsb` crate, `nsb-py` wheels
  and sdist; `nsb-cli` and `nsb-data-tools` remain unpublished workspace crates.
- A `v0.1.0` tag on the reviewed commit triggers validation, all five Python
  platform wheels, the Python sdist, crates.io publication and then PyPI
  Trusted Publishing. Neither registry upload occurs before validation.
- The public Rust API snapshot is frozen and checked in CI; release metadata
  and cross-package versions are verified before and after tagging.

### Release boundaries

- Generic clear-sky components and CTAO application-layer presets are planning
  models, not site-calibrated products.
- Starlight production data is fail-closed and requires explicit provenance,
  validation, and redistribution approval before runtime admission.
- B/V magnitudes and S10 fields are central-wavelength diagnostics, not
  validated Johnson B/V passband integrations.
- Runtime evaluation is local and deterministic for fixed inputs and admitted
  assets; catalogue acquisition and data-product generation remain offline.
