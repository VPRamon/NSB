# NSB — Observational validation

[**Open the standalone notebook**](NSB_Validation.ipynb) · [Download the notebook](https://raw.githubusercontent.com/VPRamon/NSB/main/notebooks/NSB_Validation.ipynb)

The notebook is a self-contained, executable scientific comparison of **NSB Standard**, **experimental NSB Oxygen**, **nsb2**, and a calibration-only baseline, using the registered [Patat (2008) FORS1](https://www.eso.org/~fpatat/science/skybright/) spectra. It presents observations, model predictions, error distributions, confidence intervals, component diagnostics and limitations directly in Jupyter.

**To share:** send only `NSB_Validation.ipynb`. The README, a repository checkout, helper modules and private `nsb-validation` access are not required. Public Python dependencies and astronomical datasets are installed/downloaded as needed (the notebook verifies registered checksums).

## Execute

1. Download `NSB_Validation.ipynb` and open it in Jupyter with **CPython 3.12 or 3.13**. Linux x86-64 has been tested; Google Colab and other platforms have not been verified.
2. Edit the first configuration cell. `MODE="observational"` runs the real experiment; `MODE="synthetic"` exercises the pipeline without NSB or astronomical data and **is not scientific evidence**.
3. Select the NSB runtime and **Run All**. The notebook installs public dependencies if `INSTALL_DEPENDENCIES=True`; use a fresh kernel when switching NSB versions.

### Published version

With `RUNTIME="published"`, set an exact `NSB_VERSION`. The notebook downloads a compatible PyPI wheel, verifies its hash and checks required runtime capabilities.

**Release limitation as of 24 October 2026:** `nsb-py==0.1.1` lacks the Oxygen/restricted-band API needed for a complete campaign, and `0.2.0` was not yet published. Thus the notebook's current published default deliberately **stops with an explanatory error** rather than silently substituting a model. Once a compatible release exists, select its exact version.

### Development wheel (currently needed for a full observational run)

Prepare and install an Oxygen-capable NSB wheel **outside** the notebook, using a clean source revision. One verified commit is `f5e1d22689ed417c0dcf9df72da54365c30f5bd5`. For example, from an isolated Python 3.13 environment with Rust, Cargo, Git and Maturin available:

```bash
git clone https://github.com/VPRamon/NSB.git nsb-source
git -C nsb-source checkout --detach f5e1d22689ed417c0dcf9df72da54365c30f5bd5
python -m pip install maturin==1.15.0
(cd nsb-source && python -m maturin build --release --locked --interpreter "$(which python)" --out ../wheels)
python -m pip install wheels/*.whl
sha256sum wheels/*.whl
```

Set `RUNTIME="developer"`, `NSB_VERSION`, `NSB_WHEEL` (path), `NSB_WHEEL_SHA256` (computed digest), and `NSB_COMMIT` (full source commit) in the notebook's configuration cell. Wheel bytes and the imported native extension are checked; the declared source commit is a **build-provenance assertion**, not independently inferred from a wheel. An existing external build receipt can optionally be supplied for stronger provenance. No compilation happens inside the analysis cells.

The notebook also installs **pinned public nsb2** from a Git revision, so Git and first-run network access are needed. It safely retrieves and verifies the Patat archive and measured solar F10.7 inputs, caching them for reuse.

## Compare NSB releases

Run version A and retain the generated `run-*.json` plus its `.sha256`. Start a fresh kernel using version B and set `PREVIOUS_RUN` to version A's JSON path. The notebook checks matching observational cohort, band, solar scenario, scientific calculations, reference inputs and dependencies, then displays paired B−A error differences (positive values indicate regressions). Incompatible runs are rejected.

Results and plots appear inline. Set `OUTPUT=""` to disable local result/figure export; exported files are **not** required to distribute the notebook.

## Scientific scope and interpretation

The registered experiment uses **103 Patat spectra from 22 nights** (nine calibration spectra on five nights; **94 evaluation spectra from 17 nights**) over **365–650 nm**. It evaluates native and matched measured F10.7 scenarios and uses 2,000 whole-night paired bootstrap draws, seed 182. The original research-only Starlight band proxy and physical differences between NSB and nsb2 are disclosed in the notebook.

Patat data influenced Oxygen development; the evaluation split is **not an untouched independent holdout**. Oxygen remains experimental, and no new scientific admission, generalized sky-accuracy claim, or default-model promotion follows from this notebook. Missing solar files, unsupported model APIs or invalid predictions must be resolved rather than silently skipped. Inspect residuals, uncertainty intervals, improvements **and degradations**.

This notebook ships without committed execution outputs. It does not require a separate validation package or additional repository files.
