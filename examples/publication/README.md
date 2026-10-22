# Reproduce the NSB publication example

This example evaluates the existing PALACE v1 Paranal **continuum** airglow
model over F10.7 = 67–166 sfu at a fixed public, synthetic observing scenario.
It demonstrates parameter control, component accounting and interface consistency.
It is not an observation, a calibration experiment, a complete sky model or a
claim of absolute accuracy. No random sampling or external data downloads occur
at execution time. Scientific assets are the checksum-verified bundled inputs.

## Install and run

Use Linux x86_64, Rust stable with Cargo, CPython 3.12 including its development
library, and a compiler/linker. Rust 1.89 is the declared minimum; consult the
build receipts for the actually tested toolchains. The local wheel has the
`abi3-py310` ABI but a local Linux platform tag, not a manylinux certification.

Check out the **candidate source commit listed in
[software-provenance.json](../../reports/publication-verification/software-provenance.json)**.
The evidence commit adds receipts only; the candidate commit includes all executable
code, inputs, reference outputs and documentation needed for reproduction.

```bash
python3.12 -m venv .venv
.venv/bin/python -m pip install -r examples/publication/requirements.txt
export PYO3_PYTHON="$PWD/.venv/bin/python"
.venv/bin/python examples/publication/run.py --build
```

The source wheel and CLI use the workspace's exact Siderust Git pin, whose
source must be available to Cargo during initial installation. This is not a
registry release: ordinary Cargo packaging and registry-only sdist installation
remain blocked by [#265](https://github.com/VPRamon/NSB/issues/265). Do not remove
the patch, use `--no-verify` to assert publishability, or publish this candidate.
`--build` builds a fresh wheel in `target/publication/wheels`, installs that
wheel into the calling interpreter and builds the native probe and CLI. Remove
stale wheels if deliberately changing package versions in the same checkout.

For a fast, offline rerun after installation:

```bash
.venv/bin/python examples/publication/run.py
```

The harness repeats each computation, compares all three point interfaces,
compares Rust/Python restricted bands, checks composition and partition
identities, tests zero solar flux, verifies registered results and generates
both PNG and vector PDF figures. Failed numeric comparisons are saved before
raising an error. Reference registration requires the explicit maintainer-only
`--register` flag; never use it to make a failed verification pass.

## Frozen scenario and quantities

[config/fixtures.json](config/fixtures.json) is the sole shared scientific input:
UTC `2023-09-15T05:00:00Z`, geodetic longitude −70.4045°, latitude −24.6272°,
ellipsoidal height 2635 m, ICRS RA 10°, declination −20°. This is a reproducible
scenario at Paranal coordinates, not a registered measurement. Rust applies
ICRS → Mean J2000 frame bias, as the Python/CLI input boundaries do.
Generic clear-sky atmosphere is resolved by the candidate's typed site profile;
it is not a measured atmosphere. PALACE continuum, van Rhijn thin shell (88 km),
explicit F10.7, Noll extinction and existing fixed component spectral grids are
used. There is no oxygen amendment, site recalibration or telescope response.

The principal observable is top-hat **300–650 nm photon radiance**, in
photons cm^-2 ns^-1 sr^-1. Ancillary restricted integrals use [300,450.25],
[450.25,650] and [360,570] nm. These use the model's wavelength conventions and
are not calibrated Johnson/FORS passbands. Neither B/V central-wavelength
proxies nor starlight TOA maps are presented as ground-level filter photometry.
The zero-Moon scenario is checked at the selected epoch; no nonzero lunar band
spectrum is inferred.

## Outputs and interpretation

Generated under `target/publication/results/`:

- `results.json`: native computed totals, components and four band integrals.
- `numerical-verification.json`: analytic kernel/unit/direct-path receipts.
- `interface-equivalence.json`: absolute/relative discrepancies and error checks.
- `cli-*.json`: complete unedited CLI JSON, including model and asset metadata.
- `software-provenance.json`: source, toolchains, packages and SHA256 hashes.
- `figures/airglow-sweep.{png,pdf}`: computed radiance versus F10.7.

[expected/results.json](expected/results.json) registers computed candidate
outputs; it is a regression reference, **not an independent physical oracle**.
Agreement uses `abs(a-b) <= 1e-14 + 2e-12*max(abs(a),abs(b))`, an allowance for
floating-point evaluation and serialization far below physical uncertainties.
Byte-identical figure files are not required across platforms. Numerical output
and fixture hashes are recorded separately from plot rendering.

The curve illustrates the PALACE model's prescribed solar-activity response
at one geometry. Its monotonicity is verified over the six chosen inputs; this
is not asserted for all parameters, sites or conditions. Plot points have no
physical uncertainty bars because a complete systematic uncertainty budget is
not supplied. The restricted Zodiacal+Airglow case verifies component sums;
it omits other physical sky contributions. No comparison with nsb2 or new
observational accuracy claim follows from this demonstration.

## Numerical and full engineering verification

```bash
.venv/bin/python scripts/publication-checks.py
```

This records each command, environment, exit code and log in
`target/publication/checks/`. It attempts the canonical Rust checks, installed
Python tests, example, metadata and packaging checks. Registry packaging failures
are **FAIL** with the known upstream blocker; they are not converted into PASS.
Additional clean-source, Rust-1.89 and isolated-sdist receipts are in the committed
publication evidence package. Other OS/architectures rely on the existing CI
matrix and must not be described as locally verified.
