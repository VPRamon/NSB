# NSB moonlight Mie phase grid v1

## Scientific objective and Jones context

`moonlight_mie_nsb_v1.dat` is an independently calculated, NSB-owned aerosol
phase-function table for the Jones et al. (2013) scattered-moonlight runtime. It
replaces the historical ESO-lineage `mie_m15s1.dat`; those bytes are neither a
generator input nor a published output.

The runtime keeps aerosol optical depth and angular redistribution separate:

```text
scatter = tau_R P_R(theta) + tau_M(lambda) JONES_MIE_WEIGHT P_M(theta, lambda)
```

The generated product supplies only `P_M`. Site-profile aerosol optical depth,
the empirical `JONES_MIE_WEIGHT`, and the temporarily retained historical
multiple-scattering correction remain separate runtime inputs.

## Published assumptions and NSB choices

Jones et al. (2013), Sect. 2.6 and Table 2, publish the remote-continental
lognormal modes, select real refractive index 1.5, and use relative mode amounts
100%, 45%, 5%, and 100%:

| Mode | number density (cm^-3) | modal radius (um) | log10(s) | admitted fraction |
|---|---:|---:|---:|---:|
| Tropospheric nucleation | 3200 | 0.010 | 0.161 | 1.00 |
| Tropospheric accumulation | 2900 | 0.058 | 0.217 | 0.45 |
| Tropospheric coarse | 0.300 | 0.900 | 0.380 | 0.05 |
| Stratospheric | 4.49 | 0.217 | 0.248 | 1.00 |

The paper attributes the mode parameters to Warneck & Williams (2012), states
that they are number distributions, and describes the mixture as matching the
Patat et al. (2011) Paranal extinction curve.

NSB interprets the tabulated `log s` as a base-10 logarithmic width, hence
`sigma_ln(r) = ln(10) log10(s)`, and uses constant refractive index `1.5 + 0i`.
The paper does not fully specify the log base, dispersion, a physical
large-particle cutoff, or an absorptive component. These are versioned NSB
implementation choices, not fitted parameters. No value was tuned against the
historical ESO lookup table. Jones altitude profiles affect column optical depth
and therefore are not inputs to the normalized phase-function generator.

## Solver and normalization

The in-tree Rust solver implements the spherical-particle Mie amplitude
recurrences of Bohren & Huffman (1983), equations 4.74 and 4.88. Per-particle
phase functions are combined with number density times scattering cross section,
`n(r) pi r^2 Q_sca`. The generator is AGPL-3.0-only and its only numerical
dependency here is `num-complex` (MIT/Apache-2.0); it consumes no proprietary or
unlicensed input data.

The convention is

```text
integral over 4pi of P_M(theta, lambda) dOmega = 4 pi
g(lambda) = (1 / 4pi) integral P_M cos(theta) dOmega
```

Each particle phase function is normalized analytically by the Mie coefficient
sum (`qsum`), and ensemble rows are divided by the integrated scattering weight.
Generated rows are **not** renormalized by an angular trapezoid. The independent
trapezoidal integral over the stored non-uniform grid is a validation diagnostic.
This convention matches the Jones single-scattering equation, which applies
`P/(4 pi)`.

## Production grid and radius integration

The artifact schema is `nsb-moonlight-mie-phase-v1` and contains 36 wavelengths
from 300 through 650 nm at 10 nm spacing. Its 355 scattering angles use this
piecewise grid (degrees, inclusive contiguous segments):

```text
0..0.25/0.0125; 0.25..1/0.025; 1..2/0.05; 2..5/0.125;
5..10/0.25; 10..170/1; 170..180/0.125
```

Rows are wavelength-major. Runtime evaluation is bilinear interpolation in
wavelength and angle. Each mode is integrated in `ln(radius)` with composite
Simpson quadrature over plus/minus 8 lognormal standard deviations using 1280
even intervals. The complete configuration is pinned in
`crates/nsb-data-tools/config/moonlight-aerosol-nsb-v1.toml`.

## Numerical convergence and interpolation

The ignored tests `production_radius_integration_is_converged` and
`production_quadrature_convergence` are run explicitly for production changes.
They produced:

| Comparison against production | Coverage | max pointwise phase change | max absolute change in g |
|---|---|---:|---:|
| 7 sigma / 1120 vs 8 sigma / 1280 | all 36 x 355 samples | 0.01977% | 1.18e-9 |
| 8 sigma / 640 vs 8 sigma / 1280 | all 36 x 355 samples | 1.109% | 9.96e-5 |
| 8 sigma / 1280 vs 8 sigma / 2560 | 3 wavelengths x 8 representative angles | 0.4365% | 2.72e-5 |

Using 1120 intervals at 7 sigma preserves the same steps-per-sigma as
production, so the first comparison isolates the omitted tail. It supports the
8-sigma cutoff: the additional tail has a small but measurable effect and is
included. The 640/1280 full-grid and 1280/2560 sampled comparisons quantify the
remaining radius-quadrature sensitivity. The production choice keeps measured
pointwise sensitivity below 0.5% at the explicit refinement probes and keeps `g`
stable to `3e-5`; the narrow forward structure remains the limiting numerical
region and is recorded as a v1 limitation rather than hidden by retuning.

The lifecycle validation also compares linear interpolation against direct
solver evaluations at 14 off-grid angles for 300, 480, and 650 nm. The worst
relative error is `1.781e-3` (0.1781%), below the `3e-3` gate. The refined grid
is therefore required near 0 and 180 degrees; a uniform 1-degree description is
not valid for this artifact.

## Physical and integral validation

Release-mode lifecycle validation of the committed artifact reports:

- exact model/artifact axes: 36 wavelengths x 355 angles, strictly increasing,
  with exact 300--650 nm and 0--180 degree endpoint coverage;
- finite, non-negative phase values at every sample;
- worst independent trapezoidal 4-pi normalization error: `7.371e-5`;
- grid asymmetry-factor range: `0.573705` to `0.675937`;
- worst `|grid g - coefficient g|`: `1.513e-5`;
- worst direct-solver angular interpolation error: `1.781e-3`;
- `P(0 deg) > P(90 deg)` at every wavelength.

The coefficient-derived normalization is the source of truth. The finite-grid
normalization and `g` discrepancies above measure angular sampling/integration
error and are not corrections applied to the data.

Jones Fig. 9b offers only a graphical primary-reference check. The generated
model reproduces strong forward scattering, much weaker intermediate/backward
scattering, and modest wavelength dependence. Exact curve agreement is not
claimed because the paper publishes no machine-readable curve.

## Historical ESO table: diagnostic only

Only after fixing the published model and numerical configuration was the final
artifact compared offline with the historical table from Git history. The 1,448
common samples comprise eight common wavelengths (300--650 nm at 50 nm spacing)
and all 181 integer-degree angles. The median NSB/legacy ratio is `1.2198`; the
median absolute relative difference is `27.345%`. The largest relative
difference is at 650 nm and 0 degrees: ratio `4.5334` (`112.3641` versus
`24.7858`).

Representative NSB/legacy ratios are:

| wavelength | 0 deg | 10 deg | 30 deg | 90 deg | 150 deg | 180 deg |
|---:|---:|---:|---:|---:|---:|---:|
| 300 nm | 2.6929 | 0.6697 | 1.0596 | 1.2961 | 0.7657 | 0.6368 |
| 650 nm | 4.5334 | 0.8223 | 0.8991 | 1.3245 | 1.6702 | 1.3018 |

This is diagnostic-only evidence, not a fit target or acceptance oracle. The
legacy solver, cutoff, refractive-index treatment, post-processing, and exact
provenance are insufficiently documented to attribute the differences uniquely.

## Jones 2013 end-to-end impact

With the same solar spectrum, optical-depth code, `JONES_MIE_WEIGHT`, and
historical multiple-scattering correction, replacing only the historical Mie
table changes the three regression geometries by:

| Separation | integrated 300--650 nm | B diagnostic | V diagnostic |
|---:|---:|---:|---:|
| 97.523 deg | +2.466% | +3.595% | +6.870% |
| 4.000 deg | -24.059% | -23.595% | -19.119% |
| 52.216 deg | +5.790% | +6.391% | +2.984% |

Runtime regression tests pin integrated, B, and V outputs for all three
geometries using the current analytic solar reference. The pins changed because
the final artifact adds the 5--8 sigma radius tail, doubles the Simpson sampling
density, and resolves the forward/backward angular structure on the non-uniform
grid. The close-Moon change versus the historical runtime follows from the
independently generated forward lobe and is not compensated by retuning
`JONES_MIE_WEIGHT`.

## Provenance, reproducibility, and limitations

- artifact SHA-256:
  `8ac2548e2699dee1448f60d867d4c2fd5a49b4702dba63297972e81cb3cb4bbc`;
- model/config SHA-256:
  `d63543d5b168e27669479fc0004f0a9c21f94de81b83920981e4a61c8ae24e82`.

The publish step derives machine-readable manifest header metadata from the
generated artifact. Validation independently checks artifact axes against the
pinned model. CI regenerates and validates on relevant pull requests and main
branch pushes, then fails if artifact or manifest bytes differ.

Known v1 limitations are spherical particles, constant real refractive index,
the paper's underspecified log-width notation and physical coarse-particle
cutoff, the quantified sub-0.5% sampled quadrature sensitivity, and Mie spheres'
known tendency to underrepresent nonspherical large-angle backscatter. The
historical multiple-scattering table remains a temporary planning approximation
tracked for replacement in issue #217; the pair is not claimed as a newly
calibrated radiative-transfer solution.

## Reproduction

Run in order:

```bash
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering update --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering build --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering validate --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering publish --config crates/nsb-data-tools/config/moonlight-scattering.toml
```

Repeating the lifecycle produces the hashes above and no Git diff.

## References

- Jones et al. 2013, A&A 560 A91, DOI 10.1051/0004-6361/201322433.
- Patat et al. 2011, A&A 527 A91, Cerro Paranal extinction curve.
- Warneck & Williams 2012, *The Atmospheric Chemist's Companion*.
- Bohren & Huffman 1983, *Absorption and Scattering of Light by Small Particles*.
