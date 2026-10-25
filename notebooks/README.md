# NSB notebooks

| Notebook | Purpose |
|---|---|
| [NSB_Validation.ipynb](NSB_Validation.ipynb) | Patat/FORS1 observational validation with frozen protocol, pinned dependencies and runtime provenance |
| [CTAO_API_Example.ipynb](CTAO_API_Example.ipynb) | Public NSB API across two CTAO sites, airglow model and domain examples |

Both notebooks are standalone and can be opened directly in Jupyter or Google
Colab: no checkout of VPRamon/NSB is necessary. **Edit the first code cell**,
then use **Run All** in a fresh kernel.

- `github` mode builds `nsb-py` from a **full 40-character commit SHA** at
  [VPRamon/NSB](https://github.com/VPRamon/NSB), installing Rust 1.89+ on
  Linux if required. Compilation needs network access and takes longer.
- `pypi` mode installs a specific `nsb-py` release. The package is called
  **`nsb-py`**, not `nsb`: Python code correctly uses `import nsb`.
  Do not run `pip install nsb`, which may install an unrelated package.

The examples target **v0.2.0**; the last verified PyPI release was **0.1.1**
(October 2026), which does not satisfy the notebooks' expected v0.2.0
scientific contract. Use the default pinned source commit until a compatible
release is published. No automatic fallback to an older model is performed.

The observational notebook preserves its fixed PyPI-wheel SHA-256 verification,
or checks the actual PEP 610 GitHub commit and installed native files when
built from a Git revision. Its `nsb2` comparison and scientific data integrity
checks remain separate.

Automatic Airglow in v0.2.0 selects experimental PALACE Oxygen for zenith
distance ≤60°. Other sites are spatial extrapolations; invalid geometry raises
rather than silently selecting continuum. The scientific model is not
independently calibrated. See [scientific validation](../docs/scientific-validation.md).
