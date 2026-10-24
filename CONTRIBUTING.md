# Contributing to NSB

NSB is scientific software. Changes must preserve both numerical correctness
and the accuracy of scientific claims.

## Before submitting a pull request

1. Describe the scientific quantity, units, reference frame, input domain and
   affected component. State whether the change alters predictions.
2. Add a regression test or independently generated numerical/observational
   fixture where appropriate. Do not use the implementation under test to
   generate its own sole expected values.
3. Separate numerical verification from astrophysical/observational validation.
   Do not promote a model's maturity based on a passing implementation test.
4. Preserve scientific data provenance, licence terms and SHA-256 identities.
   Never modify checksum-approved assets without following the redistribution
   and scientific-admission procedures.
5. Update user-facing examples, scientific limitations and API documentation
   whenever behaviour or applicability changes.

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
python scripts/check-release-metadata.py
```

For packaging and publishing use
[maintainer release procedures](docs/maintainer-guide/releases.md).
For the correct scope of scientific conclusions see
[scientific validation](docs/scientific-validation.md). Open an issue for
proposed changes to scientific assumptions before rewriting validated data.
