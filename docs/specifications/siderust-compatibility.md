# Siderust compatibility

Status: Current dependency compatibility and update policy.
Audience: Maintainers, downstream packagers, and dependency reviewers.
Scope: Siderust dependency source, lockfile package identity, update rules, and release
requirements.
Non-goals: Scientific model admission and empirical accuracy claims.

## Current State

| NSB | Siderust package | Manifest source | Public source identity | Rust MSRV | Status |
| --- | --- | --- | --- | --- | --- |
| 0.2.0 preparation; experimental transport | 0.14.1 | SHA-pinned workspace Git patch | `git:https://github.com/Siderust/siderust@83d1ebb29ca1ae0b327d5784d13eda231d52c120` | 1.89 | Temporary; registry publication blocked (#265) |

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

### Registry release blocker

Registry Siderust 0.15.0 supplies the required optics APIs, but uses
Tempoch 0.11/Affn 0.12, while NSB and siderust-py 0.4
require the older time/coordinate providers. A registry probe with Siderust 0.15.0 and direct Tempoch 0.11.1 fails with
13 `E0308` type errors in NSB Python adapters, including observer conversion and
`tempoch_py::interop::period_to_datetimes_with`. The bridges still require the
older provider identities. Release requires compatible Python bindings, API verification,
unpatched registry package verification and installed-wheel/sdist checks.

### Dependency upgrade boundaries for 0.2.0

Registry metadata checked on 23 October 2026:

| Dependency | Selected | Latest stable | Reason for selection |
| --- | --- | --- | --- |
| Siderust | 0.14.1 + immutable Git patch | 0.15.0 | Registry Python bridge requires Siderust 0.14 / Affn 0.11; 0.15 requires Affn 0.12 and Tempoch 0.11. |
| siderust-py / tempoch-py | 0.4.0 | 0.4.0 | Latest published interoperability bridges. |
| Tempoch / core | 0.10.0 | 0.11.1 | Must share time types with tempoch-py 0.4 and the selected Siderust. |
| PyO3 | 0.29.3 | 0.30.1 | Both bridges require 0.29; mixed pyo3-ffi versions conflict on `links = "pyo3-python"`. |
| Principia / Optica | 0.6.0 / 0.4.0 | 0.7.0 / 0.5.0 | Transitive Siderust providers; migrate with the coherent astronomy graph. |
| Qtty / core | 0.8.6 | 0.8.6 | Latest; consumed through upstream re-exports. |
| Reqwest | 0.12.28 | 0.13.5 | Preserve the existing Rustls/ring and bundled WebPKI root policy. 0.13 replaces `rustls-tls` with `rustls`, platform certificate verification and AWS-LC; changing trust roots/build prerequisites needs a separate networking review. Upstream siderust-archive also uses 0.12. |

MD5 0.11 (Rust 1.85) and toml_edit 0.25 (Rust 1.85) fit NSB's Rust 1.89
minimum. MD5 now shares Digest 0.11 with SHA-256; toml_edit shares the TOML
1.1 parser/datetime stack. ZIP 9.0.5 retains its Rust 1.88 minimum and deflate
feature selection. Compatible transitive updates are recorded in Cargo.lock.
No scientific provider is changed by these upgrades.

The Python distributions `siderust` and `tempoch` remain at their latest 0.4.0
line. Maturin 1.15.0 is the latest tested backend and stays pinned in PEP 517
and CI. NumPy is not an NSB runtime or Cargo dependency: generator/reference
versions and checksummed oracle metadata remain historical evidence. Upgrading
the Rust numpy 0.30 crate would require PyO3 0.30 and is not needed by NSB.

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
