# NSB notebooks

| Notebook | Purpose |
|---|---|
| [NSB_Validation.ipynb](NSB_Validation.ipynb) | Standalone Patat/FORS1 observational demonstration with frozen protocol, explicit provenance and figures |
| [CTAO_API_Example.ipynb](CTAO_API_Example.ipynb) | Short public-API example: observer configuration, components, Automatic/Oxygen versus continuum, and domain errors |

The API example uses an installed NSB build, Siderust, Astropy, NumPy and
Matplotlib; it needs no downloaded scientific dataset or private helpers.
Build this release candidate with `python -m pip install .`, then install
`astropy numpy matplotlib ipykernel` and run all cells in a fresh kernel.

The observational notebook documents its own pinned environment and developer-wheel
verification. Follow its setup cells; a source wheel is required until a compatible
release is published. Its synthetic mode exercises the pipeline without observational
evidence. The full observational mode actually computes figures and comparisons.

Automatic resolves to Experimental PALACE Oxygen in v0.2.0. Its ZD ≤60° limit
applies to planning as well as point evaluation; invalid inputs raise errors without
fallback. Explicit `ParanalPalaceV1` retains historical continuum behavior.
Patat informed model development and is not an independent holdout. LVM absolute
equivalence remains pending; neither notebook grants scientific admission.

For a concise statement of what these experiments support—and what they do not
support—read the [scientific validation guide](../docs/scientific-validation.md).
The observational notebook identifies its data provenance and reproducible
comparison protocol within the notebook itself. External datasets are not
bundled with the software distribution.
