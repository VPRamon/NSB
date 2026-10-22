# Release reproducibility and external blocker

## Issue #265: still externally blocked

Read in full (including its empty comment thread). Its cause remains on inspected
main `57584ee298f25f8af7a6f169d71e87f8a59eea9a`: registry Siderust **0.14.1** does
not contain `LayeredAtmosphere` and `NormalizedPhaseTable`, while the exact Git
revision `83d1ebb29ca1ae0b327d5784d13eda231d52c120` does. Registry metadata was
queried during investigation and still reported 0.14.1 as newest. Upstream HEAD
was `b9c26689e2fe72b869ecae9c87594fca34dbc5a3`; this is recorded only as a remote
identity, not substituted for the tested pin.

No NSB-only dependency edit can create the absent upstream registry release.
The smallest correct action is to retain the existing exact patch, single type
identity, Git-source policy exception, negative archive contract and tag guard.
Ordinary Cargo archive verification removes the workspace patch and must fail;
`--no-verify` is not publishability. No upstream code change, fork or new
registry version is invented. Follow-up: upstream publishes a release with both
APIs compatible with siderust-py, then update **every** workspace requirement,
remove patch/exception/negative helper, regenerate Cargo.lock and execute the
unmodified package/publish dry-run plus full matrix.

Rust and CLI repository builds use the pin. The PyO3 wheel builds from that
same patched workspace; runtime Python `siderust==0.4.0` and `tempoch==0.4.0`
are installed packages, while compiled bindings share one Rust Siderust identity.
The Maturin sdist preserves the workspace patch and can be built independently
with access to the pinned Git source. That is source reproducibility, **not a
registry-only dependency chain** or authorization to publish PyPI. Release
policy intentionally blocks tagging until #265 is resolved.

## Software identity and build receipts

Authoritative crate and Python package version: **0.1.1**; model contract
`nsb-model-v1`; CLI/data-tool crates 0.1.1. Versions alone are insufficient to
identify this unpublished candidate. Use `software-provenance.json`'s full
source SHA, Cargo.lock SHA256 and pinned dependencies. qtty/qtty-core/qtty-derive
0.8.6 and Optica 0.4.0 are registry dependencies with checksums in Cargo.lock;
the same lockfile records the complete graph. Cargo `--locked` is used for
repository builds. The library-only sdist uses Maturin's pruned lockfile, whose
archive identity is recorded separately.

`verification-results.json` records commands, exit codes, environments and log
paths. Initial environment failures are retained in the engineering-history
receipt: system Python 3.14 lacks libpython; managed Python 3.12 needs its LIBDIR
on the Linux loader path for all-feature **test executables**. The checks driver
sets this path using sysconfig. This does not change the scientific model.

Only Linux x86_64 is locally exercised. The existing release workflow remains
responsible for the macOS/Windows, Linux ARM, abi3 and manylinux matrices; no
remote CI result should be asserted before it finishes. The built local wheel's
actual platform tag is recorded, not promoted to an untested compatibility tag.
No public tag, registry publication or merge is performed.

See [reproduction instructions](../../examples/publication/README.md) and
[claim integration](manuscript-evidence.md). Failed release checks leave the
release blocker **OPEN**, regardless of successful repository-source builds.

## Recorded reproduction environment and artifact access

Primary: Rust/Cargo **1.97.1**, Python **3.12.13**, Linux x86_64, source
`a2ae1274f401a56e73d87350b41d025a5fff0928`, Cargo.lock SHA256
`693716afa2f23b8aa749e98378f40d11feae720614c152b7ab771be60fb2d875`.
Rust **1.89.0** also compiles all workspace targets/features with the same locked
crate/package inputs. The source commit retains the existing asset and release
guards; no Cargo manifest, lockfile or physical model was changed by this PR.
The only scientific arithmetic change is the documented diagnostic hc correction.

Use the exact Python transitive pins in `python-environment.txt` when reproducing
the recorded environment (install those first, then run the example's `--build`).
Select Rust 1.97.1 with rustup to reproduce the primary toolchain; the actual
executed commands use the active toolchain recorded in the manifest. Source,
input, archive, wheel and installed-binary hashes are in software-provenance.json.
The wheel/sdist binaries remain generated local artifacts under `target/`; their
hashes identify the tested distributions without committing platform binaries.
The data/figures and verification logs are committed.

The source archive was exported with `git archive <candidate>` into a fresh
directory with no build target, then the documented `run.py --build` was executed.
It rebuilt all native artifacts, installed its new wheel, recomputed all results
and regenerated the figure. The CLI runs from a separate temporary directory.
A separate source-installed CLI, fresh Python environment and isolated PEP 517
sdist wheel were also exercised, with commands/exit codes retained. Installed
artifacts do not need source-side scientific data files: those data are embedded.
