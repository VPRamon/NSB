# NSB release automation

Release from a validated, immutable **vX.Y.Z** tag after registry-package
preflight and scientific approval. The tagged commit must have successful full
CI from main, including API, coverage, MSRV, scientific and installed-wheel gates.
Require the corrected published siderust-py and Python siderust >=0.5.1,<0.6;
0.5.0 contains the query safety defect
([upstream #13](https://github.com/Siderust/siderust-py/issues/13)) and is excluded
by the Rust/Python requirements and release metadata preflight.
Registry packaging compatibility alone does not establish release readiness.

## v0.2.0 scientific release-scope decision

Before tagging v0.2.0, the maintainer must explicitly approve publication as
**reproducible scientific planning software**, not as an absolute, calibrated
night-sky predictor. The v0.2.0 Automatic Airglow choice is experimental
PALACE Oxygen, based on a Paranal-derived continuum, with zenith distance
**at most 60 degrees** under its documented standard geometry. Unsupported
directions fail explicitly; the explicit continuum-only model covers a
different domain and is not a silent fallback.

Independent held-out absolute predictive accuracy, global Airglow portability,
and CTAO observatory calibration are **not admitted claims** of this release.
The generic-core/site-calibration redesign (#200) and final Airglow audit
(#154) remain follow-up work; their unresolved broader objectives must not
be presented as completed by passing CI. Read
[scientific validation](../scientific-validation.md) and
[model maturity](../specifications/model-maturity.md), and confirm the release
notes agree with runtime metadata. A release may proceed only if the maintainer
accepts this narrow scientific scope and the exact commit passes the gates.

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
CHANGELOG.md, rejecting missing, duplicate or empty sections. The resulting GitHub Release is titled **NSB vX.Y.Z**, with those notes and
six assets. The workflow re-fetches the release and fails if its tag, title,
notes, draft/prerelease status, asset inventory or sizes differ from the
validated files (also checking SHA-256 where GitHub reports asset digests).
An existing, identical GitHub Release is left untouched; a mismatching one
is never overwritten.

**A successful registry upload is not release completion.** Require the
`Publish GitHub Release` job to finish with `success` (not `skipped`),
verify the public GitHub Release for the exact tag, and check crates.io and
PyPI availability. If publication is partial, use the recovery procedure
below instead of moving the tag or rerunning its publication jobs.

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
