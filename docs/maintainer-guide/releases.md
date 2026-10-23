# NSB release automation

Release from a validated, immutable **vX.Y.Z** tag after registry-package
preflight and scientific approval. Do **not** push tags while ordinary
Cargo registry packaging is blocked by #265.

The tag-triggered workflow validates metadata, API compatibility, runtime
scientific assets, licensing and crate packaging; builds five ABI3 wheels
(Linux x86_64/aarch64, macOS x86_64/arm64, Windows x86_64) and one sdist;
publishes Rust nsb to crates.io, then nsb-py to PyPI; and validates the
scientific publication inventory without changing approvals.

Publishing jobs target the **existing** GitHub environments **crates.io** and
**pypi**, with registry deployment URLs. The Cargo registry identifier remains
crates-io (not the environment name). The crates.io job uses the existing
CARGO_REGISTRY_TOKEN secret; PyPI uses its configured **Trusted Publisher**,
GitHub OIDC (id-token: write), and no PyPI API token. Deployment permissions
are job-local. Do not recreate environments or change credentials.

After successful publication and scientific recording, the final job checks
the exact six Python artifacts, platform/ABI and embedded metadata, confirms
the crates.io version, and compares SHA-256 hashes of all six files with PyPI.
It extracts the matching **## [X.Y.Z]** section from the *tagged*
CHANGELOG.md, rejecting missing, duplicate or empty sections. The resulting
GitHub Release is titled **NSB vX.Y.Z**, with those notes and six assets.
An existing GitHub Release is left untouched.

## Recovery and safe reruns

A tag rerun cannot republish immutable registry artifacts: preflight
intentionally refuses already-published PyPI versions. Prefer
**workflow_dispatch** on main, providing the original wheels run ID,
validated sdist run ID, release tag and recovery mode. Both IDs must refer to
push runs of that tag. They may be the **same successful run** for
GitHub-Release-only recovery, but must be **distinct** when recovering a
missing sdist. Source ancestry and the narrow packaging-only change
allowlist are verified; original wheels must match existing PyPI hashes.

- **pypi-sdist** (default): the five wheels exist on PyPI, the sdist does not.
  Publish only the validated sdist, re-run scientific recording, then create
  the missing GitHub Release.
- **github-release**: both registries and all six PyPI files already exist.
  Check all six bytes against PyPI and the crate against crates.io, re-run
  scientific recording and create only the missing GitHub Release. No registry
  publication job runs.

The flows reuse the existing provenance checks and reject unexpected changes
or artifacts. Both triggers share a non-cancelling version-specific
concurrency group, avoiding competing release creation. Existing Releases
are never overwritten.

Offline checks: python -m unittest discover -s scripts/tests -p
'test_release_assets.py'. CI also runs actionlint on the publish workflow.
Do not publish registry packages, create real Releases, or push release
tags for regression testing.
