# Changelog

All notable changes are recorded here. The project follows semantic versioning
once a stable public release is cut.

## Unreleased

### Changed

- Made library `ALL`, library `DEFAULT`, and CLI `all` the same production-safe
  three-component set.
- Renamed the bundled starlight path as an experimental seed and made access
  explicitly opt-in.
- Cached parsed starlight and shared airglow calibration state inside
  `NsbEvaluator`.
- Pinned Siderust 0.10.1 to Git revision
  `8d94b8375ae23c26d00346f74951e52cd1b595cc`.
- Expanded JSON and CSV with version, model, maturity, provenance, uncertainty,
  band-diagnostic, and asset-checksum metadata.

### Added

- Versioned scientific asset manifest and checksum/header verifier.
- Independent published KS91 validation fixture with units and tolerance.
- Point/component/window benchmarks and scheduled/manual benchmark workflow.
- Formatting, clippy, locked tests, doctests, docs, release build, MSRV, license,
  advisory, source, and stale-API CI gates.
- Model maturity, roadmap, CLI schema, compatibility, and release documents.

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
