# Maintainer guide

The [reproducible dataset workflow](datasets.md) is the single operational
entry point for scientific asset update, build, validation, local/Slurm
execution, recovery and publication.

Maintainers must keep software correctness separate from scientific maturity.
Reproducing a historical snapshot does not resolve missing provenance,
licensing, calibration or independent validation.

Before a release run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
cargo deny check
```

Coverage floors in [`coverage-policy.toml`](../../coverage-policy.toml) are
blocking on `main`. See [Coverage policy](../developer-guide/coverage.md) and
the [release checklist](../operations/release-checklist.md).

Runtime assets are authoritative in `crates/nsb/data/manifest.toml`. Dataset
configuration and run manifests provide generation identity; scientific
maturity remains authoritative in runtime metadata and
`docs/specifications/model-maturity.md`.
