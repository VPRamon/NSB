# Starlight Gaia XP continuous tools

This directory contains documentation only. The former Python and shell workflows were removed by issue #61.

The supported implementation lives in `crates/nsb-data-tools` and consists of compiled Rust capabilities:

- `normalize_xp_continuous_coefficients`
- `reconstruct_canonical_coefficients`
- `validate_xp_continuous_reconstruction`
- `download_gaia_xp_continuous_bulk`
- `index_gaia_xp_continuous_bulk`

Build the release binaries with Cargo, then invoke the required binary directly. No retained workflow depends on a Python environment, GaiaXPy at runtime, shell wrappers, `cargo run` orchestration, or sibling executable chaining.

Scientific contract, frozen parity evidence, and provenance requirements are documented in [`docs/nsb_components/starlight/gaia-xp-continuous-rust.md`](../../docs/nsb_components/starlight/gaia-xp-continuous-rust.md).
