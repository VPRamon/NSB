# Changelog

Release availability on crates.io and PyPI is confirmed only after the
tag-triggered publication workflow succeeds.

## [Unreleased]

## [0.2.0]

Release candidate; registry availability depends on the tag-triggered
publication workflow completing successfully.

### Added

- Component-separated restricted-band radiance for supported 300–650 nm
  component spectra in Rust and Python. Unsupported spectral contributions
  produce explicit errors rather than inferred zeros.
- Registered Airglow calibration interfaces and an experimental PALACE
  continuum-plus-oxygen emission model with reproducible asset provenance.
- Versioned spectral Starlight map ingestion, native bins and uncertainty
  channels, with explicit top-of-atmosphere applicability limits.
- Opt-in experimental atmospheric transport references with documented
  numerical verification. These are not part of default NSB evaluations.
- Reproducible scientific reports and cross-interface examples with validation
  evidence and limitations stated separately.

### Changed

- Automatic Airglow selects `ParanalPalaceOxygenV1`. The model is
  **Experimental** and supports standard continuum geometry at zenith distances
  no greater than 60°. Outside that domain evaluation fails explicitly.
  Select `ParanalPalaceV1` for the distinct continuum-only model.
- Public coordinate/time/photometry boundary types use published Siderust
  0.15.1 and Tempoch 0.11.1. Python requires compatible `siderust`
  (>=0.5.1,<0.6) and `tempoch` (>=0.5.0,<0.6).
- The release gate verifies scientific assets, licenses, CI status,
  crate packaging, Python wheels and source distribution before publishing.

### Fixed

- S10 photon-energy conversion uses exact SI `h*c`. This changes only S10
  diagnostics by approximately 2e-14 relative, not integrated radiance.
- Starlight spectral-map parsing rejects unsupported geometry, unavailable
  photometry and inconsistent uncertainty/covariance inputs explicitly.
- Scientific asset packaging preserves source-specific terms and
  checksum-pinned provenance independently of the AGPL software license.

### Compatibility and scientific scope

- Rust consumers must use the coordinated Siderust/Tempoch public type versions.
- The full four-component model has **not** been independently validated for
  absolute observational night-sky radiance. CTAO presets are planning models;
  Starlight implementation agreement is not absolute astrophysical validation.
  See [scientific validation](docs/scientific-validation.md) and
  [model maturity](docs/specifications/model-maturity.md).

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
