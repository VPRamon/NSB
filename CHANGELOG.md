# Changelog

All notable changes are recorded here. Published versions follow semantic
versioning; the `0.x` series remains pre-1.0.

## 0.1.0 - Unreleased

First public NSB release.

### Added

- Added the typed Rust `nsb` runtime for point night-sky-background evaluation
  and threshold-window planning over the 300–650 nm integrated photon-radiance
  contract.
- Added the `nsb-rust` Python distribution (Python 3.10+, import name `nsb`)
  with PyO3 `abi3-py310` wheels. Python uses canonical
  `siderust.Observer` / `siderust.Direction` objects and tempoch's Python
  datetime bridge rather than NSB-owned coordinate/time compatibility types
  (#210).
- Added the `nsb-cli` workspace application with point/window evaluation,
  observatory lookup, stable table/JSON/CSV output, configuration templates,
  logging, and explicit separation between physical observatory location and
  scientific site profile.
- Added zodiacal light, airglow, Jones 2013 and KS91 scattered moonlight, and
  map-backed integrated-starlight component contracts with per-component
  provenance, maturity, validation-domain, uncertainty, and diagnostic
  metadata.
- Added generic typed site profiles through `SiteProfileTag` and
  `SiteProfile<P>`. CTAO North/South remain application-layer planning
  presets and do not become core observatory identities or calibration claims
  (#185).
- Added `nsb::transport`, a minimal atmospheric-transport foundation for
  origin-checked spectral identity and direct Beer–Lambert transmission over
  Siderust Rayleigh/Mie/ozone/airmass primitives (#187, #193).
- Added the Rust-only `nsb-data` maintainer executable and reproducible
  scientific-asset lifecycle for acquisition, transformation, validation,
  reconciliation, checksums, provenance, and packaging.
- Replaced the provenance-unresolved historical Airglow continuum snapshot with
  a compact, deterministic PALACE v1.0 CC-BY-4.0 continuum product preserving
  separate HO2, FeO-like, and unresolved O2 spectra plus month, local-time,
  solar-activity, and residual-variability semantics (#214).
- Added the frozen nside-128 combined 300–650 nm Starlight candidate with the
  covariance-corrected bright-star supplement, deterministic runtime staging,
  checksum-pinned technical and external cross-implementation validation, and
  fail-closed production admission (#207, #211). Redistribution/production
  activation remains tracked separately in #103 and does not block the
  `0.1.0` MVP because candidate/runtime bytes are excluded from publishable
  packages.
- Added a Rust Galactic Mollweide Starlight heatmap example tied to exact
  candidate bytes by SHA-256 (#212).
- Added versioned scientific-asset manifests, checksum/header verification,
  independent KS91 fixtures, end-to-end scientific tests, coverage policy,
  scheduled/manual benchmarks, and public API snapshot tooling.
- Added a tag-driven release workflow that validates `vMAJOR.MINOR.PATCH`
  against package metadata, builds Python wheels for Linux/macOS/Windows plus
  an sdist, publishes `nsb` to crates.io, and publishes `nsb-rust` to PyPI
  with Trusted Publishing/OIDC from the same commit (#197).

### Changed

- Standardized the Rust workspace on the published
  `siderust = "0.12.0"` crates.io dependency, `tempoch = "0.7"`, and the
  public provenance identity `crates.io:siderust:0.12.0`.
- Consumed qtty/Optica API types through Siderust's public re-exports so NSB
  cannot accidentally compile against incompatible instances of those domain
  types.
- Reworked observatory handling onto Siderust `ObservatoryCatalog`, with NSB
  catalog extensions for CTAO-N/S, H.E.S.S., MAGIC, FACT, VERITAS, FAST, GTC,
  and deterministic external-catalog replacement semantics (#140).
- Made `ComponentMask::DEFAULT`, `ComponentMask::ALL`, and CLI
  `--components all` the same production-safe composition. Unapproved
  Starlight is never silently substituted into the default.
- Replaced the former Rust coverage-gate crate with
  `scripts/coverage-gate.sh` while preserving blocking workspace/core/diff
  coverage floors.
- Replaced the historical Moonlight Mie and multiple-scattering lookup bytes
  with NSB-owned reproducible products generated from documented Jones-model
  assumptions and an in-tree radiative-transfer pipeline (#220, #222).
- Minimized and reviewed the first-release Rust public API, marked evolvable
  records/enums/errors non-exhaustive where appropriate, and froze the final
  `0.1.0` surface after the PALACE v1.0 Airglow identity correction.

### Scientific status and release boundaries

- Generic clear-sky components and CTAO application-layer presets are planning
  models, not site-calibrated products.
- The repository contains a scientifically/technically reviewed Starlight
  release candidate, but the `0.1.0` package does not redistribute or activate
  it as bundled production data while #103's human redistribution/licensing
  decision is pending.
- Validated external Starlight maps remain available only through the explicit,
  fail-closed sidecar admission contract.
- B/V magnitudes and S10 fields are central-wavelength diagnostics, not
  validated Johnson B/V passband integrations.
- Runtime evaluation is local and deterministic for fixed inputs and admitted
  assets; catalogue acquisition and data-product generation remain offline.
