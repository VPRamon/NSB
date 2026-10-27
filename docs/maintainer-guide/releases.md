# NSB release automation

Release from a validated, immutable **vX.Y.Z** tag after registry-package
preflight and scientific approval. The tagged commit must have successful full
CI from main, including API, coverage, MSRV, scientific and installed-wheel gates.
Require published siderust-py and Python siderust >=0.6.0,<0.7;
0.5.0 contains the query safety defect
([upstream #13](https://github.com/Siderust/siderust-py/issues/13)) and is excluded
by the Rust/Python requirements and release metadata preflight.
Registry packaging compatibility alone does not establish release readiness.

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

## Preparing v0.3.0

The release preparation PR targets v0.3.0 because horizontal targets introduce
intentional public Rust field and Python direction type changes since v0.2.0.
See the [changelog](../../CHANGELOG.md) and [Python migration](../python.md).
The reviewed target migration contract permits only its exact four API
substitutions when comparing with an older base; comparisons against a migrated
base retain strict SemVer checks. No workflow exception is needed for v0.3.0.

Use `python scripts/check-release-metadata.py --tag v0.3.0` and the full CI
checks before merging the preparation PR. Keep `nsb-py`'s Maturin version
dynamic; `nsb-cli` and `nsb-data-tools` remain unpublished. Installation examples
for the eventual publication are `cargo add nsb@0.3.0` and
`python -m pip install nsb-py==0.3.0`.

Only after the preparation PR is merged, required scientific approvals are in
place, and full CI succeeds for the exact resulting main commit may maintainers
create the release tag. Preparing this PR does not authorize publishing or
creating that tag. Historical scientific artifacts are not evidence of a rerun
on v0.3.0.

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
'test_release_*.py'. CI also runs actionlint on the publish workflow.
Do not publish registry packages, create real Releases, or push release
tags for regression testing.
