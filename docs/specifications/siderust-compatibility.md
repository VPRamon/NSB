# Siderust compatibility

Status: Current dependency compatibility and update policy.
Audience: Maintainers, downstream packagers, and dependency reviewers.
Scope: Siderust dependency source, lockfile package identity, update rules, and release
requirements.
Non-goals: Scientific model admission and empirical accuracy claims.

## Current State

| NSB | Siderust package | Manifest source | Public source identity | Rust MSRV | Status |
| --- | --- | --- | --- | --- | --- |
| 0.1 line; single-scattering reference | 0.14.1 | SHA-pinned workspace Git patch | `git:https://github.com/Siderust/siderust@83d1ebb29ca1ae0b327d5784d13eda231d52c120` | 1.89 | Temporary; registry publication blocked (#265) |

All three workspace crates retain the compatible `siderust = "0.14.1"` requirement.
The root `[patch.crates-io]` redirects every consumer, including `siderust-py`, to
Git revision `83d1ebb29ca1ae0b327d5784d13eda231d52c120` (verified main; upstream
PRs #108/#110). The lockfile contains exactly one Siderust package identity.
`crates/nsb` and `crates/nsb-cli` request the `atmosphere` and
`photometry` features, while `crates/nsb-data-tools` uses its own feature set.
The version/source identity is shared even though feature selection is not
identical.

`Cargo.lock` records the complete immutable Git source/revision, not a registry checksum.
The workspace also uses Tempoch `0.10.0`, and both Python interoperability
crates (`siderust-py` and `tempoch-py`) use `0.4.0`.

### Registry 0.15.0 audit (#275)

Siderust 0.15.0 is published and contains the optics APIs, but replacing only
NSB's direct requirement is not a compatible migration. It introduces Tempoch
0.11/Affn 0.12 while NSB's direct time dependency and siderust-py 0.4 retain the
older type providers. `cargo check --workspace --all-targets --all-features
--locked` fails with incompatible time/coordinate identities. The attempted
upgrade has therefore been deferred and the known-compatible immutable graph
restored. This preserves truthful exported provenance and the public API.

Resolving #265 still requires compatible upstream Python bindings, a reviewed
time/coordinate migration, an API comparison against v0.1.1, ordinary unpatched
registry package verification, installed-wheel interoperability and isolated
sdist verification. Do not remove the negative package contract or Git-source
exception before those requirements pass.

### Scientific update boundaries

Siderust 0.14.1 supplies the typed lunar irradiance API and corrected
inverse-square source dilution and observer-local Rayleigh columns. NSB
preserves dimensional boundaries for optical depth, phase functions,
irradiance-to-radiance conversion, spectral densities, and integrated
photon radiance. The Jones scattering column and Noll zodiacal equations
retain their scientific tests and limitations in the component
[Moonlight](../nsb_components/moonlight/scattering-column-correction.md)
and [Zodiacal](../nsb_components/zodiacal/noll-equation-correction.md)
references.

A dependency bump is not assumed to preserve radiances or observing windows:
rerun the independent physical-boundary and planning regression suites.
Completed migration logs, before/after snapshots and fixture-reconstruction
notes are historical material and belong in Git history, not in this
current compatibility policy.

### Python package versioning

The Python runtime dependency named `siderust` is the Siderust Python
distribution and currently uses the independent `0.4.x` version line:

```text
nsb-py -> siderust>=0.4.0,<0.5
         -> tempoch>=0.4.0,<0.5
```

This is not a mismatch with the Rust `siderust = "0.14.1"` dependency. The
Python extension uses `siderust-py` for cross-extension interoperability,
while the Rust scientific implementation links the Rust crate shown in the
matrix above.

Public library exports `nsb::SIDERUST_VERSION` and `nsb::SIDERUST_SOURCE` must
match this matrix. A workspace contract test fails if the declared dependency,
lockfile package, or published provenance constants disagree.


## Release Requirement

**crates.io/PyPI releases are blocked until [#265](https://github.com/VPRamon/NSB/issues/265)
replaces the temporary patch with a real upstream registry release.** Ordinary
`cargo package` creates an archive but its isolated verification resolves registry
0.14.1 and fails on missing layered/normalized-phase imports. The temporary CI
helper verifies this exact negative case and then compiles the extracted archive
with the explicit pin. This does not establish publishability. Tag metadata
preflight fails closed while the patch remains. Repository wheels and sdists
retain the pin; their build compatibility is tested separately.

Transition: update all workspace version requirements and Cargo.lock, verify
`siderust-py` still shares one type identity, remove the patch/source exception
and temporary package verifier, restore ordinary package checks, rerun Rust 1.89,
Python wheels, isolated sdist build and release dry-run. Do not invent a version.

A release that claims reproducible dependency resolution must satisfy all of:

1. every workspace manifest uses the same Siderust source;
2. `Cargo.lock` is committed and matches that source;
3. `cargo deny check` passes with the locked graph;
4. CLI version metadata and this matrix report the same dependency identity;
5. Siderust-dependent validation fixtures are rerun.

## Why Updates Need Review

NSB relies on Siderust for time scales, coordinates, ephemerides, atmosphere
primitives, event searches, HEALPix support, Gaia/passband preparation, and
starlight-map validation helpers. The `nsb::transport` layer reuses Siderust
airmass, Rayleigh/Mie optical depth, Beer–Lambert transmission, ozone
transmittance, and phase-function primitives rather than re-implementing them.
A Siderust update can change scientific outputs without changing NSB public API
shapes.

## Update Procedure

1. Review upstream Siderust release notes and relevant scientific/data API
   changes.
2. Choose a released Siderust version or exact Git commit for release builds.
3. Keep `crates/nsb`, `crates/nsb-cli`, and `crates/nsb-data-tools` on the same
   dependency source.
4. Run `cargo update -p siderust` and review `Cargo.lock`.
5. Update `nsb::SIDERUST_VERSION` and `nsb::SIDERUST_SOURCE` to match the
   resolved package (and keep this matrix / CLI expectations in sync).
6. Rerun release gates, scientific validation tests, starlight admission tests,
   and CLI schema tests.
7. Update root README dependency text when present and `CHANGELOG.md`.
