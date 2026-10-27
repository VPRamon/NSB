# Changelog

Release availability on crates.io and PyPI is confirmed only after the
tag-triggered publication workflow succeeds.

## [Unreleased]

## [0.3.0]

Horizontal targets, scientific diagnostics and opt-in evaluation capabilities
added since v0.2.0. This minor release includes intentional Rust and Python API
changes; production coefficients and default broadband model prescriptions are
unchanged.

### Added

- Opt-in Rust `NsbEvaluator::evaluate_spectral_starlight` for complete native
  Starlight bins in the experimental 360–570 nm domain, composing explicit
  observer geometry and atmospheric identity, direct, single-scattering or
  projected multiple-scattering modes. Keep top-of-atmosphere, transmitted and
  scattered quantities separate; partial/unavailable totals and physical
  uncertainties remain unavailable rather than zero (#312).
- Typed Oxygen F10.7 domain diagnostics in Rust, Python and CLI JSON: applied
  solar flux and the asset-derived 67–166 sfu training interval. Numerical
  membership does not certify physical conditions or scientific maturity (#309).
- Deterministic observational validation receipts with manifest, model/scenario
  predictions, per-night results, condition groups and source checksums,
  reproducible independently of the notebook (#307).
- Native horizontal (Alt/Az) targets across Rust, Python, CLI and planning:
  `TargetDirection` / `HorizontalTarget`, canonical Python `ICRS` / `Horizontal`
  directions, and CLI `--alt`/`--az` (#314).

### Planning and numerical robustness

- Resolve fixed horizontal pointings to equatorial coordinates at each instant;
  evaluate dynamic Starlight under Earth rotation and time-dependent Zodiacal
  regimes. Adaptive discovery partitions moving targets at Galactic HEALPix
  transitions and authoritative physical regimes (#314).
- Adaptive HEALPix discovery is not mathematically certified. Certified search
  continues to reject unsupported dynamic Starlight certificates rather than
  assuming constant Starlight for horizontal targets (#314).

### Rust, Python and CLI interfaces; breaking changes

- **Breaking (Python):** replace `siderust.Direction` with
  `from siderust.direction import ICRS, Horizontal`. Construct equatorial targets
  with `ICRS(ra_deg=..., dec_deg=...)` or fixed horizontal pointings with
  `Horizontal(alt_deg=..., az_deg=...)`; both use the existing `target` argument
  on `PointQuery` / `ThresholdQuery`. Requires `siderust>=0.6.0,<0.7` and the
  published Rust `siderust-py` 0.6.x bridge (protocol 2) (#314).
- **Breaking (Rust public fields):** `PointQuery::target` and
  `ThresholdQuery::target` now have type `TargetDirection`, representing either
  an equatorial `Target` or a `HorizontalTarget`. Constructors accept
  `Into<TargetDirection>`, preserving calls that pass existing equatorial
  targets. Struct literals, direct field assignments and code reading fields
  as `Target` must migrate using explicit conversion or enum matching. The
  reviewed four-signature migration is recorded in the API compatibility
  contract; unrelated SemVer protections remain strict (#314).
- Retain published Rust Siderust 0.15.1, Tempoch 0.11.1 and PyO3 0.30.1;
  Python Tempoch remains `>=0.5.0,<0.6`. Python `nsb-py` dynamically inherits
  version 0.3.0 from the core Cargo manifest. All three workspace packages are
  synchronized; `nsb-cli` and `nsb-data-tools` remain unpublished.

### Scientific validation and reproducibility

- Night-clustered signed-bias intervals, paired differences, night-weighted
  errors and predeclared solar/airmass condition summaries for the historical
  Patat/FORS1 development cohort; prevent pooled statistics from obscuring
  between-night variation (#293).
- Reproduce prospective-versus-retrospective F10.7 cadence diagnostics using
  checksum-verified scientific inputs. Smaller historical errors from trailing
  or previous-month solar proxies do not justify changing runtime defaults;
  centred-window inputs depend on future observations (#308).
- Add matched continuum and O I 557.7/630/636 nm window diagnostics, including
  air/vacuum wavelength conventions and integrated line-flux operators; retain
  the frozen cohort and distinguish developmental from independent evidence
  (#310).
- Verify matched NSB–nsb2 Starlight operators to floating-point accuracy in
  synthetic controlled cases, while documenting differences in real catalog
  populations and angular sampling. This does not demonstrate astrophysical
  superiority or full-sky accuracy (#311).

### Fixed and clarified

- Keep Automatic Oxygen's planning release-policy fallback distinct from its
  experimental scientific status and the independent absolute admission gate
  (#302).
- Restore standalone execution of the observational notebook using released
  Python packages and a clean Jupyter setup (#284).
- Ensure tag-triggered publication validates successful main-branch CI for the
  exact release commit, allowing that CI to finish before checking status (#288).

### Scientific limitations

- The default four-component model remains a planning model, without new
  independent absolute astrophysical validation. Automatic PALACE Oxygen
  remains **Experimental**; numerical agreement does not establish
  observational accuracy. Spectral Starlight transport remains opt-in;
  production coefficients and approved scientific assets are unchanged.
- Historical Oxygen error changes were measured on data that informed model
  development, not an independent absolute admission sample. The full
  sky-to-detector accuracy experiment remains blocked by missing matched
  calibration and spectral coverage. CTAO site profiles remain planning
  presets, not calibrated instrument predictions.
- The separately maintained `nsb-validation` PRs #27 and #28 preserve public
  measurement provenance and instrument-admission NO-GO decisions; those
  observational data fixtures are **not** newly bundled in this NSB release.

## [0.2.0]

Release candidate; registry availability depends on the tag-triggered
publication workflow completing successfully.

### Added

- Component-separated restricted-band results and single-pass evaluation in Rust
  and Python, with explicit unsupported contributions and uncertainty semantics
  (#271).
- Registered Airglow calibration workflow and PALACE oxygen emission model;
  development diagnostics do not establish independent accuracy (#272).
- Standalone observational demonstration and focused CTAO public-API example
  under `examples/`; complete experiments remain in `nsb-validation`.
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
