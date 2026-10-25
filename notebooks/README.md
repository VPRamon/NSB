# NSB notebooks

| Notebook | Purpose |
|---|---|
| [NSB_Validation.ipynb](NSB_Validation.ipynb) | Scientific assessment of nsb-py against Patat/FORS1 observations, nsb2 and a calibration-only baseline |
| [CTAO_API_Example.ipynb](CTAO_API_Example.ipynb) | Practical nsb-py 0.2.0 guide: point radiance, components, Airglow and observation windows at both CTAO sites |

Both notebooks are standalone: no local checkout or private service is required.
They can be opened in Jupyter or Google Colab with a compatible Python runtime.
The distribution is **`nsb-py`**; Python code uses `import nsb`. Do not install the
unrelated package named `nsb`.

## Scientific assessment

`NSB_Validation.ipynb` begins with the scientific questions and observing regime,
then presents the comparison design, measured errors and paired uncertainties,
physical diagnostics, and conclusions. Input checks and reproducibility details
are explained in a technical appendix; supporting code is folded where supported.
The notebook is distributed without saved outputs. Use **Restart and Run All**
to generate the tables, figures and conclusions in order.

Prepare a CPython **3.12 or 3.13** Jupyter kernel:

```bash
python -m pip install nsb-py==0.2.0 nsb2 numpy scipy astropy matplotlib siderust tempoch
```

The small configuration cell identifies existing cached inputs. Scientific
settings define the registered experiment and should remain unchanged for a
reproduction. Network access is needed for the initial acquisition of Patat
spectra and nsb2 reference assets; subsequent runs reuse cached inputs. Installed
versions and scientific input hashes are displayed in the appendix. The notebook
uses the installed packages and does not install them or build Rust automatically.

The assessment retains 103 spectra from 22 Paranal nights over 365–650 nm, with
94 evaluation spectra on 17 nights. Native and matched solar-input scenarios
are reported separately. Patat informed Oxygen development: this is development
evidence, not an independent test of generalization or CTAO site calibration.
The restricted-band Starlight contribution uses an explicit research-only
spectral proxy. See [scientific validation](../docs/scientific-validation.md).

Execution displays tables and figures inline and does not export JSON, checksum
or image files. Observations, predictions, metrics and failure details remain in
kernel memory. Patat and solar input hashes and the registered sample identity
are still verified. Missing inputs are downloaded using Astropy's cache; existing
files under `CACHE` are reused after verification. FITS spectra are read directly
from the TAR without extracting a directory.
An unavailable or invalid scenario is identified explicitly; an incomplete
campaign does not produce an overall comparative conclusion.

## CTAO API example

`CTAO_API_Example.ipynb` supports **nsb-py 0.2.0 from PyPI** with CPython
**3.12 or 3.13**. Prepare the kernel environment before running:

```bash
python -m pip install nsb-py==0.2.0 numpy matplotlib astropy
```

Use **Restart and Run All** in a fresh kernel. The notebook does not install
packages, acquire external data or export files. Code that teaches the public
API remains visible; results are generated inline without saved outputs.

The guide starts with one explicit point query toward the Crab Nebula, compares
total radiance and components at both CTAO array locations, explores Airglow
versus zenith distance, and searches for UTC intervals below an illustrative
radiance ceiling. Observer coordinates use the project's WGS84 array positions;
atmospheric profiles remain planning assumptions. All examples use fixed,
editable dates and solar inputs in the default 300–650 nm band.

Automatic Airglow in v0.2.0 selects experimental PALACE Oxygen with standard
continuum geometry and zenith distance ≤60°. The default examples stay inside
this domain. The site profiles and the illustrative threshold do not establish
observational calibration or detector performance; consult `NSB_Validation.ipynb`
for the separate scientific assessment.
