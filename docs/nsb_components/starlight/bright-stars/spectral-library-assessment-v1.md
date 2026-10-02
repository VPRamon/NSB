# Bright-star spectral-library assessment v1

Status: experimental evidence; no redistribution approval.

| Library | Coverage | Parameter/type coverage | Calibration | Machine access | Redistribution posture | v1 decision |
|---|---|---|---|---|---|---|
| CALSPEC | Source-dependent; coverage must be checked per file | Sparse standards, not a population library | HST absolute spectrophotometry | STScI CALSPEC FITS/ASCII archive | External validation input; terms and file identity must be pinned | Validation subset only |
| BOSZ 2024 (corrected 2025-09-24 snapshot) | 50 nm–32 μm | 2800–16000 K, broad log(g), metallicity, alpha, carbon, microturbulence grid | Synthetic surface flux (MARCS/ATLAS9 + Synspec), not an absolute observed flux | MAST HLSP, predictable machine-readable files and wavelength grids | External checksum-pinned build input pending #103 review | Preferred template candidate inside its parameter domain |
| Castelli–Kurucz 2004 | UV–IR model grid | Broad hot-star grid, interpolatable | Synthetic surface flux | STScI reference-atlas tarball | External checksum-pinned build input pending #103 review | Candidate for temperatures not covered by BOSZ; not yet selected |
| PHOENIX | UV–IR, grid-dependent | Strong cool-star coverage | Synthetic surface flux | Large STScI/PHOENIX grids | Terms and exact snapshot need a separate audit | Not selected for v1 |
| Pickles 1998 | Wide observed/composite spectral coverage | 131 spectral-type/luminosity templates | Relative flux-calibrated composites | STScI reference atlas | Exact redistribution terms/origin components need review | Comparison candidate only; not selected because nsb2 uses it |

Authoritative technical routes:

- BOSZ MAST HLSP: <https://archive.stsci.edu/hlsp/bosz>
- STScI reference atlases: <https://archive.stsci.edu/hlsp/reference-atlases>
- CALSPEC archive: <https://ssb.stsci.edu/cdbs/calspec/>

The build must reject a source when its evidence does not select a template
inside that library's declared parameter domain. Colour-only inference is a
separate lower-confidence route and must carry a larger empirical/model
uncertainty; it is not silently promoted to spectral-type template evidence.
