# Examples

This directory contains the Python notebooks and indexes the runnable examples.
Rust sources live in [`crates/nsb/examples/`](../crates/nsb/examples/) and are
compiled by the repository CI.

## Rust

Run these from the repository root with Rust 1.89+:

| Example | Purpose | Source |
| --- | --- | --- |
| `point_query` | Evaluate NSB for one target and UTC instant | [`point_query.rs`](../crates/nsb/examples/point_query.rs) |
| `threshold_window` | Find periods below a radiance threshold | [`threshold_window.rs`](../crates/nsb/examples/threshold_window.rs) |
| `model_metadata` | Select components and inspect model provenance | [`model_metadata.rs`](../crates/nsb/examples/model_metadata.rs) |

```bash
cargo run --locked -p nsb --example point_query
cargo run --locked -p nsb --example threshold_window
cargo run --locked -p nsb --example model_metadata
```

The example programs use the local workspace dependencies and pinned lockfile.
For library installation in your own project, see the
[main README](../README.md#install) and [crates.io](https://crates.io/crates/nsb).

## Python

The [Python quickstart](../README.md#python) shows a complete point evaluation
using `nsb-py` (import `nsb`) and `siderust`. Continue with the
[Python guide](../docs/python.md) for threshold-window planning, model
selection, errors and units.

### Notebooks

| Notebook | Purpose |
|---|---|
| [observational-validation.ipynb](observational-validation.ipynb) | Scientific assessment of nsb-py against Patat/FORS1 observations, nsb2 and a calibration-only baseline |
| [ctao-sky-background.ipynb](ctao-sky-background.ipynb) | Practical nsb-py 0.3.0 guide: point radiance, components, Airglow and observation windows at both CTAO sites |
| [lst1-night-sky-background.ipynb](lst1-night-sky-background.ipynb) | ORM/CTAO-North capabilities demo: equatorial vs native horizontal targets, components and ThresholdQuery planning in both frames |
| [airglow-solar-activity.ipynb](airglow-solar-activity.ipynb) | Controlled F10.7 sweep of PALACE continuum Airglow at fixed Paranal geometry |

All notebooks are standalone: no local checkout or private service is required.
They can be opened in Jupyter or Google Colab with a compatible Python runtime.
All setup cells pin **`nsb-py==0.3.0`** and **`siderust==0.6.0`** from PyPI.
The distribution is **`nsb-py`**; Python code uses `import nsb`. Do not install the
unrelated package named `nsb`.

### Scientific assessment

`observational-validation.ipynb` begins with the scientific questions and observing regime,
then presents the comparison design, measured errors and paired uncertainties,
physical diagnostics, and conclusions. Input checks and reproducibility details
are explained in a technical appendix; supporting code is folded where supported.
The notebook includes saved tables and figures for inspection directly on GitHub.
Use **Restart and Run All** to regenerate the results in order.

Select a fresh CPython **3.12 or 3.13** Jupyter/Google Colab kernel and **Run All**.
The first code cell installs exactly **`nsb-py==0.3.0` from PyPI**, together with
the notebook's scientific Python dependencies, using the current kernel's Python.
No NSB checkout, GitHub installation of NSB or Rust toolchain is needed; installation
requires a compatible NSB wheel. Matching installed packages are reused. If setup
changes an already imported package, restart the kernel and Run All as instructed.
The editable `REQUIREMENTS` list shows the packages being installed.

The separate **nsb2 reference** is installed from GitHub commit
`bc9320db03fbff69997ce366c6e76a37339c5d00`, since it is not published on PyPI;
Git and network access to GitHub are required for this reference only.
The existing `CACHE` configuration selects local scientific inputs. Registered
scientific settings should remain unchanged for reproduction. Public observations
and nsb2 reference assets are downloaded initially and reused from verified caches.
Installed versions and scientific input hashes are displayed in the appendix.

The assessment retains 103 spectra from 22 Paranal nights over 365–650 nm, with
94 evaluation spectra on 17 nights. Native and matched solar-input scenarios
are reported separately. Patat informed Oxygen development: this is development
evidence, not an independent test of generalization or CTAO site calibration.
The restricted-band Starlight contribution uses an explicit research-only
spectral proxy. See [scientific validation](../docs/scientific-validation.md).

Execution displays tables and figures inline. The appendix also writes versioned
`manifest.json`, `metrics.json` and `predictions.csv` audit receipts to
`observational-validation-receipts/` relative to the kernel working directory
(configurable as `RECEIPTS`). See the [receipt contract and independent metric
reconstruction procedure](../docs/observational-receipts.md). A complete, aligned
two-scenario campaign is required for export; no raw spectra are copied.
Observations, predictions, metrics and failure details also remain in kernel
memory. Patat and solar input hashes and the registered sample identity
are still verified. Missing inputs are downloaded using Astropy's cache; existing
files under `CACHE` are reused after verification. FITS spectra are read directly
from the TAR without extracting a directory.
An unavailable or invalid scenario is identified explicitly; an incomplete
campaign does not produce an overall comparative conclusion.

### CTAO API example

`ctao-sky-background.ipynb` supports **nsb-py 0.3.0 from PyPI** with CPython
**3.12 or 3.13**. Use a fresh kernel and **Run All**: its first code cell installs
`nsb-py==0.3.0`, NumPy, Matplotlib and Astropy using the kernel's Python.
It requires no local checkout, GitHub access or Rust build. If setup changes
already imported packages, restart the kernel and Run All as instructed.
Code that teaches the public API remains visible; results are generated inline.
The notebook does not acquire external scientific data or export files.

The guide starts with one explicit point query toward the Crab Nebula, compares
total radiance and components at both CTAO array locations, explores Airglow
versus zenith distance, and searches for UTC intervals below an illustrative
radiance ceiling. Observer coordinates use the project's WGS84 array positions;
atmospheric profiles remain planning assumptions. All examples use fixed,
editable dates and solar inputs in the default 300–650 nm band.

Automatic Airglow in v0.3.0 selects experimental PALACE Oxygen with standard
continuum geometry and zenith distance ≤60°. The default examples stay inside
this domain. The site profiles and the illustrative threshold do not establish
observational calibration or detector performance; consult `observational-validation.ipynb`
for the separate scientific assessment.

### Continuum solar-input example

[airglow-solar-activity.ipynb](airglow-solar-activity.ipynb) varies six
explicit F10.7 inputs from 67 to 166 sfu at a fixed Paranal observer, ICRS direction
and UTC instant. It selects PALACE continuum only and integrates Airglow over
300–650 nm, displaying a radiance table and figure. This illustrates a controlled
model response, not observational validation.

Select a fresh CPython **3.12 or 3.13** kernel and **Run All**. The first code
cell installs `nsb-py==0.3.0` from PyPI and Matplotlib using the kernel's Python,
without a checkout, GitHub access or Rust build. If setup changes already imported
packages, restart the kernel and Run All as instructed. The notebook uses bundled
NSB assets, requires no scientific data acquisition and exports no files.

`lst1-night-sky-background.ipynb` demonstrates native `siderust.direction.Horizontal`
alongside ICRS targets at the Roque de los Muchachos / CTAO-North planning
coordinates. Its first code cell installs the published `nsb-py==0.3.0` and
`siderust==0.6.0` wheels together with the plotting dependencies. No local build
is required. It does not claim LST-1 observational validation.

The notebooks retain their saved outputs and execution counts so
readers can inspect the scientific results without running the code. The existing
documentation gate, `python scripts/check-product-docs.py`, checks notebook
structure, Python syntax, setup ordering, the NSB version selection and local
documentation links; saved outputs are permitted. It runs in the existing CI job.

## Command line

The [CLI quickstart](../README.md#cli) demonstrates point evaluation with
the `nsb` executable. The [getting-started guide](../docs/user-guide/getting-started.md)
covers observing-window searches, JSON/CSV output, named sites and custom
coordinates.

**Note:** `nsb-cli` is a workspace crate and is not published separately
to crates.io in the 0.2 release line.

### Registered Airglow calibration

`cargo run --locked -p nsb --example airglow_calibration` generates a runnable,
checksum-linked synthetic fit request and portable artifact. The example recovers
one continuum scale without promoting scientific maturity. See
[the complete observatory workflow](../docs/user-guide/airglow-calibration.md).

`point_query` supplies the minimal evaluation path; `model_metadata` demonstrates
explicit component/model selection and provenance. Custom observer coordinates
use the public Siderust observer type shown in the CLI/Python quickstarts; they
do not establish site calibration. `airglow_calibration` is an advanced synthetic
fit/export/replay example, not an admitted observatory calibration. The generic
global Airglow model remains unavailable: automatic selection reports its
explicit PALACE reference fallback and applicability in component metadata.
