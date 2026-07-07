# Changelog

All notable changes are recorded here. The project follows semantic versioning
once a stable public release is cut.

## Unreleased

### Changed

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

- NSB-side Gaia DR3 starlight release pipeline harness: documented Gaia
  extraction recipe, Gaia XP passband source preparation, Gaia photon-flux
  HEALPix map generation path, validation report command, and candidate asset
  packer. The real bundled production asset remains pending real Gaia
  extraction and independent validation.
- Build-script plumbing for the Gaia DR3 bundled production starlight CSV/TOML:
  exactly one registered production release pair is checksum-embedded and loaded
  through the runtime validated-map contract; absent assets fail closed.
- Versioned scientific asset manifest and checksum/header verifier.
- Independent published KS91 validation fixture with units and tolerance.
- Point/component/window benchmarks and scheduled/manual benchmark workflow.
- Formatting, clippy, locked tests, doctests, docs, release build, MSRV, license,
  advisory, source, and stale-API CI gates.
- Model maturity, roadmap, CLI schema, compatibility, and release documents.
- Fail-closed `ValidatedExternalMap` API and CLI path with checksum, provenance,
  header, HEALPix, flux-evidence, plane/pole, seam, photometry, and independent-
  comparison admission checks.

### Removed

- `ComponentMask::ALL_SUPPORTED`.
- `NsbEvaluator::python_parity` and `NsbModelConfig::python_parity`.
- `NsbEvaluator::periods_below_threshold_legacy` and its production benchmark.
- Obsolete historical implementation reports that contradicted the current
  architecture; history remains available through Git.

### Scientific limitations

- The bundled starlight file remains a manual, incomplete experimental seed.
- Starlight B/V-to-integrated conversion remains an explicitly named proxy.
- CTAO profiles remain planning presets pending cleared site calibration data.
- Several inherited atmospheric assets lack recoverable upstream release and
  license metadata; the manifest records this as a promotion blocker.
