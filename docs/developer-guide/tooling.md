# Repository tooling policy

Status: Active maintainer guidance.
Audience: Contributors and maintainers.

## Ownership

- `.github/workflows/` owns CI/CD orchestration.
- `scripts/` contains only small Bash entry points for CI/CD checks and release
  orchestration. Prefer invoking standard tools directly instead of adding
  wrappers; do not put scientific transformations here.
- `crates/nsb-data-tools/` owns offline scientific-data acquisition, generation,
  processing, validation, and packaging. Reuse the existing `nsb-data` CLI and
  Rust modules rather than introducing one-off crates.
- New complex maintenance logic belongs in Rust only when an existing standard
  tool cannot reasonably do the job; avoid custom code if the tool itself
  would be more maintenance than the check.

## Environment

Keep repository maintenance independent of Python. Simple repository checks
should use Bash; nontrivial reusable functionality should use Rust. Python
remains necessary for Python bindings, wheels, installed-wheel tests, and
certain externally supplied scientific formats.

One **transitional scientific exception** remains:
`crates/nsb-data-tools/tools/build_starlight_bright_star_spectral_model.py`
reconstructs a checksum-pinned CK04/Hp model using NumPy and Astropy. It
is not a CI entry point or a runtime dependency. It was moved out of
`scripts/` so the ownership boundary is explicit, but it must not be
removed until a tested Rust implementation reproduces its model output,
checksums, template selection and floating-point tolerances. When porting,
prefer a pure-Rust FITS reader to an extra native library dependency.

## Protected CI policies

The public API gate, Rust coverage floors and changed-production-line
coverage are release contracts, not disposable helpers. Simplify their
implementation only when the replacement preserves the same failure
semantics, including exclusions and handling of missing instrumentation.
The `#[cfg(test)]` boundary check is kept because the diff coverage
classifier relies on it. Keep focused regression tests for any custom
policy check that survives.
