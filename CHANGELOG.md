# Changelog

All notable changes to published NSB releases are recorded here.

## 0.1.0 - 2026-10-07

First public NSB release.

### Added

- Typed Rust `nsb` runtime for point night-sky-background evaluation and
  threshold-window planning over the 300–650 nm integrated photon-radiance
  contract.
- `nsb-rust` Python distribution (Python 3.10+, import name `nsb`) using
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
  technical validation and fail-closed production admission. Candidate/runtime
  bytes are not redistributed or activated by the 0.1.0 package.
- Versioned scientific-asset manifests, checksum/header verification,
  independent scientific fixtures, end-to-end tests, coverage policy,
  scheduled/manual benchmarks, and public API snapshot tooling.
- Tag-driven release workflow for `vMAJOR.MINOR.PATCH` that validates package
  metadata, builds Python wheels and an sdist, publishes `nsb` to crates.io,
  and publishes `nsb-rust` to PyPI using Trusted Publishing/OIDC.

### Release boundaries

- Generic clear-sky components and CTAO application-layer presets are planning
  models, not site-calibrated products.
- Starlight production data is fail-closed and requires explicit provenance,
  validation, and redistribution approval before runtime admission.
- B/V magnitudes and S10 fields are central-wavelength diagnostics, not
  validated Johnson B/V passband integrations.
- Runtime evaluation is local and deterministic for fixed inputs and admitted
  assets; catalogue acquisition and data-product generation remain offline.
