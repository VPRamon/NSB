# Siderust compatibility

Status: Registry dependency baseline for NSB 0.2.0; corrected Python bridge 0.5.1.
Audience: Maintainers and downstream packagers.
Scope: Provider identities, interoperability, provenance and release requirements.
Non-goals: Scientific admission or observational accuracy claims.

## Registry graph

| Provider | Locked release | Source |
| --- | --- | --- |
| Siderust | 0.15.1 | crates.io |
| Tempoch / core | 0.11.1 | crates.io |
| Affn / derive | 0.12.1 / 0.12.0 | crates.io, through upstream providers |
| Qtty / core | 0.8.6 | crates.io, through Siderust re-exports |
| Principia / Optica / Keplerian | 0.7.0 / 0.5.0 / 0.5.1 | crates.io, through Siderust |
| siderust-py / tempoch-py | 0.5.1 / 0.5.0 | crates.io |
| PyO3 / pyo3-ffi | 0.30.1 | crates.io; one linking identity |
| Python siderust / tempoch | 0.5.1 / 0.5.0 | PyPI; `siderust>=0.5.1,<0.6`, `tempoch>=0.5.0,<0.6` |

The coordinated 0.5 bridges resolve the former Affn/Tempoch/PyO3 mismatch.
Siderust 0.15.1 is the published documentation-fix patch over 0.15.0, which
contains the required layered-atmosphere and normalized-phase APIs. All workspace
Siderust requirements are `=0.15.1`: this exact provider pin keeps the exported
scientific provenance truthful even when a downstream Cargo build resolves
without NSB's workspace lockfile. No direct Affn or Qtty dependency is added.
Rust 1.89 and Python 3.10 remain the supported minima; wheels retain `abi3-py310`.
Downstream Rust applications must update their domain providers to this graph;
0.14/0.10 type identities cannot be mixed with NSB 0.2.0. This dependency identity
transition is documented even though the NSB public API snapshot has no drift.

The root Git patch, Git-source exception and negative packaging helper are
removed. Ordinary Cargo package verification, publish dry-run and compilation
of extracted archives use registry dependencies without injected configuration.
The isolated sdist likewise needs neither a Git checkout nor a dependency patch.
Cargo.lock records registry checksums for every non-workspace package.

`NSB_VERSION` remains the package version; `MODEL_VERSION` remains the independent
model-composition contract. Public provenance is `SIDERUST_VERSION = "0.15.1"`
and `SIDERUST_SOURCE = "registry:crates.io/siderust@0.15.1"`. Rust/CLI/Python tests
check these exports. Release preflight rejects patches, Git sources, missing
checksums and duplicate provider identities; Cargo's locked checks enforce
manifest/lock agreement. No public API snapshot is changed merely for this upgrade.

## Python ownership and safety

NSB uses `siderust_py::interop` for canonical Observer/Direction objects and
`tempoch_py::interop` for UTC datetimes and periods. PyO3 0.30 makes by-value
class extraction opt-in; removing obsolete `skip_from_py_object` attributes
preserves NSB's existing class behavior. Computational methods retain GIL release
and the existing error translation. No replacement wrapper classes or unsafe
casts are introduced.

Published siderust-py and Python siderust **0.5.1** correct
[upstream #13](https://github.com/Siderust/siderust-py/issues/13): 0.5.0 cast
`PyObject` header pointers to Rust Star/Direction payload references and dropped
the borrow guards. The correction stores `PyRef` guards in `Subject` and retains
them throughout every query operation, without unchecked casts or clones. It also
rejects non-finite search-window endpoints. The published crates.io source and
PyPI sdist were compared with the merged
[fix #14](https://github.com/Siderust/siderust-py/pull/14); installed PyPI wheels
pass the former Direction reproduction and Star/Direction method parity checks.

NSB's primitive-only official interop protocol does not call that dispatcher.
Existing installed NSB tests cover canonical classes, UTC offsets, subsecond
round trips, periods and invalid inputs. The Cargo requirement `0.5.1` and Python
requirement `>=0.5.1,<0.6` exclude affected 0.5.0 even without this workspace's
lockfile. Release preflight independently rejects a lower manifest minimum,
unsafe locked bridge, or old Python bounds. No upstream implementation is copied
into NSB, and no unsafe workaround is added.

## Scientific and release boundaries

The astronomy graph changes public provider versions, so existing coordinate,
UTC/time-scale, lunar irradiance, Rayleigh/Mie/phase, HEALPix, observing-window,
restricted-band and independent transport regressions must pass with unchanged
oracles and tolerances. Siderust 0.15 adds satellite/orientation functionality;
those additions are not new NSB scientific models. The 0.15.1 patch itself fixes
documentation and metadata. Existing physical defaults, scientific assets,
checksum-pinned approvals and historical evaluated revisions are preserved.

Experimental single/multiple scattering remains opt-in. Generic Airglow model
admission and independent CTAO calibration are not implied by a registry migration.
Retain the component [Moonlight](../nsb_components/moonlight/scattering-column-correction.md),
[Zodiacal](../nsb_components/zodiacal/noll-equation-correction.md),
[model maturity](model-maturity.md) and [validation](validation.md) caveats.

Before tagging, review exact distribution sources, pass the complete CI and
registry packaging gates, then obtain the
required maintainer/scientific/redistribution approvals. The release workflow
requires successful main-branch CI for the exact tagged commit, including coverage,
MSRV, API, numerical and cross-platform installed-wheel checks. Existing crates.io
credentials, PyPI trusted publishing, environment deployments and recovery checks
remain intact. See [release procedure](../maintainer-guide/releases.md).

## Update policy

Keep all workspace provider requirements, lockfile identities, provenance
constants/tests and this matrix consistent. Future upgrades require the same
scientific, MSRV, API, Cargo archive, installed wheel and isolated sdist gates.
Reqwest stays at 0.12.28 to preserve the existing Rustls/ring and bundled WebPKI
root policy; its 0.13 trust-store/AWS-LC migration remains separate. Maturin
1.15.0 remains pinned. NumPy/SciPy versions in reference-generator receipts are
historical evidence and are not rewritten by a runtime dependency migration.
