# Siderust compatibility

Status: Registry dependency baseline for NSB 0.2.0; upstream Python safety blocker.
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
| siderust-py / tempoch-py | 0.5.0 | crates.io |
| PyO3 / pyo3-ffi | 0.30.1 | crates.io; one linking identity |
| Python siderust / tempoch | 0.5.0 | PyPI; supported requirements `>=0.5.0,<0.6` |

The coordinated 0.5 bridges resolve the former Affn/Tempoch/PyO3 mismatch.
Siderust 0.15.1 is the published documentation-fix patch over 0.15.0, which
contains the required layered-atmosphere and normalized-phase APIs. All workspace
Siderust requirements are `=0.15.1`: this exact provider pin keeps the exported
scientific provenance truthful even when a downstream Cargo build resolves
without NSB's workspace lockfile. No direct Affn or Qtty dependency is added.
Rust 1.89 and Python 3.10 remain the supported minima; wheels retain `abi3-py310`.

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

**NOT READY — remaining upstream safety blocker:** published siderust-py and
Python siderust 0.5.0 retain invalid casts from a `PyObject` header pointer to a
Rust Star/Direction payload in `src/queries.rs`. This is reachable through public
free query functions; a published-wheel reproduction gives different altitude
results from the safe Direction method. See
[upstream #13](https://github.com/Siderust/siderust-py/issues/13) for the source
analysis, reproduction and proposed safe PyRef-guard fix. Current upstream main
was also affected when inspected on 24 October 2026. NSB's primitive-only bridge
does not call that dispatcher, but passing NSB tests does not establish safety of
the runtime dependency as a whole. Corrected published distributions, source
review and renewed interoperability checks are required before release approval.
No upstream repository or package publication is changed by this migration.

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

Before tagging, resolve the upstream safety blocker, review exact distribution
sources, pass the complete CI and registry packaging gates, then obtain the
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
