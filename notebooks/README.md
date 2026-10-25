# NSB notebooks

| Notebook | Purpose |
|---|---|
| [NSB_Validation.ipynb](NSB_Validation.ipynb) | Scientific assessment of nsb-py against Patat/FORS1 observations, nsb2 and a calibration-only baseline |
| [CTAO_API_Example.ipynb](CTAO_API_Example.ipynb) | Public NSB API across two CTAO sites, airglow model and domain examples |

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
python -m pip install nsb-py==0.2.0 nsb2 numpy astropy matplotlib siderust tempoch
```

The small configuration cell controls the cache and output directory. Scientific
settings define the registered experiment and should remain unchanged for a
reproduction. Network access is needed for the initial acquisition of Patat
spectra and nsb2 reference assets; subsequent runs reuse cached inputs. Installed
versions and scientific input hashes are recorded. The notebook uses the installed
packages and does not install them or build Rust automatically.

The assessment retains 103 spectra from 22 Paranal nights over 365–650 nm, with
94 evaluation spectra on 17 nights. Native and matched solar-input scenarios
are reported separately. Patat informed Oxygen development: this is development
evidence, not an independent test of generalization or CTAO site calibration.
The restricted-band Starlight contribution uses an explicit research-only
spectral proxy. See [scientific validation](../docs/scientific-validation.md).

By default, execution exports `validation-results/latest.json`, its detached
SHA-256. Figures are displayed in the notebook and are not exported as separate
files. Set `OUTPUT = ""` to disable the record export as well.
An unavailable or invalid scenario is identified explicitly; an incomplete
campaign does not produce an overall comparative conclusion.

## CTAO API example

`CTAO_API_Example.ipynb` has its own setup workflow. Edit its first code cell,
then use **Run All** in a fresh kernel. Its default `github` mode builds from a
full 40-character commit SHA; it requires Rust 1.89+ and can install Rust on Linux.
Its `pypi` mode selects an exact compatible release. Restart the kernel before
changing the source.

Automatic Airglow in v0.2.0 selects experimental PALACE Oxygen with standard
continuum geometry and zenith distance ≤60°. Other sites are spatial
extrapolations; invalid geometry raises rather than silently selecting continuum.
The CTAO coordinates and profiles are planning examples, not observational
calibrations.
