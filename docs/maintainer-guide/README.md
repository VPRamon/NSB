# Maintainer guide

The [reproducible dataset workflow](datasets.md) is the single operational
entry point for scientific asset update, build, validation, local/Slurm
execution, recovery and publication.

Maintainers must keep software correctness separate from scientific maturity.
Reproducing an existing snapshot does not resolve missing provenance,
licensing, calibration or independent validation.

See [release automation](releases.md) for tag publication, deployment
records, Trusted Publishing and safe recovery.

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
blocking on `main`. See [Coverage policy](../developer-guide/coverage.md).

Runtime assets are authoritative in `crates/nsb/data/manifest.toml`. Dataset
configuration and run manifests provide generation identity; scientific
maturity remains authoritative in runtime metadata and
`docs/specifications/model-maturity.md`.

## Repository responsibilities

NSB owns scientific software: Rust/Python/CLI code, numerical regression
fixtures, runtime assets, API/user/developer documentation, compact examples,
licensing, packaging and release automation. Keep CI results in CI artifacts;
do not commit successful execution logs or manuscript evidence dossiers.

NSB publishes the scientific claims and limitations of the current software in
[scientific-validation.md](../scientific-validation.md), backed by local
numerical tests, validation fixtures and checksum-pinned evidence.
Third-party observational datasets may have separate distribution conditions.
Where evidence was produced externally, preserve immutable producer identities,
input digests and the comparison protocol: do not silently relabel historical
model evaluations as validation of a newer release.

Keep manuscript-specific experiments and intermediate outputs separate from
the product's user documentation. A numerical regression fixture stays with NSB
whenever it protects a supported scientific behaviour.
