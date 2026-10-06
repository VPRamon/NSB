# NSB moonlight Mie phase grid v1

## Scientific objective and Jones context

`moonlight_mie_nsb_v1.dat` is an independently calculated, NSB-owned aerosol
phase-function table for the Jones et al. (2013) scattered-moonlight runtime.
It replaces the historical ESO-lineage `mie_m15s1.dat`; those bytes are neither
an input nor a generation target.

The runtime keeps aerosol optical depth and angular redistribution separate:

```text
scatter = tau_R P_R(theta) + tau_M(lambda) JONES_MIE_WEIGHT P_M(theta, lambda)
```

`tau_M` therefore remains the site-profile extinction law. This product only
supplies `P_M`; it does not encode aerosol column density or optical depth.
The still-historical multiple-scattering correction is a separate multiplicative
table and is tracked for replacement in #217.

## Published assumptions and independent choices

Jones et al. (2013), Sect. 2.6 and Table 2, explicitly publish remote-continental
lognormal modes and select real refractive index 1.5 with relative mode amounts
100%, 45%, 5%, and 100%. The admitted model is:

| Mode | number density (cm^-3) | modal radius (um) | log10(s) | admitted fraction |
|---|---:|---:|---:|---:|
| Tropospheric nucleation | 3200 | 0.010 | 0.161 | 1.00 |
| Tropospheric accumulation | 2900 | 0.058 | 0.217 | 0.45 |
| Tropospheric coarse | 0.300 | 0.900 | 0.380 | 0.05 |
| Stratospheric | 4.49 | 0.217 | 0.248 | 1.00 |

The paper attributes the mode parameters to Warneck & Williams (2012), states
that they are number distributions, and describes the selected mixture as the
combination matching the Patat et al. (2011) Paranal extinction curve. Jones
uses a single refractive index for the selected reconstruction; no wavelength
dependence or absorptive imaginary part is published for this calculation.

NSB interprets the paper's tabulated `log s` as base-10 logarithmic width, so
the standard deviation in ln(radius) is `ln(10) log10(s)`. It uses a constant
complex refractive index `1.5 + 0i`. These are explicit implementation choices
where the paper does not specify log base or dispersion. No value was tuned to
the historical LUT. The altitude profiles in Jones control column optical depth,
not the normalized phase function, and are consequently not generator inputs.

## Solver, equations, and licensing

The generator is a small Rust implementation of the spherical-particle Mie
amplitude recurrences in Bohren & Huffman (1983), equations 4.74 and 4.88. It is
part of `nsb-data-tools` under AGPL-3.0-only and uses `num-complex` 0.4.6
(MIT/Apache-2.0) for complex arithmetic. There is no external solver or input
dataset. Per-particle intensities are combined using number density times Mie
scattering cross section, `n(r) pi r^2 Q_sca`, before normalization.

The convention is

```text
integral over 4pi of P_M(theta, lambda) dOmega = 4 pi
g(lambda) = (1 / 4pi) integral P_M cos(theta) dOmega
```

This matches the Jones single-scattering equation, which applies `P/(4 pi)`.
The generated rows are renormalized with the same 1-degree trapezoidal angular
integral used by validation, making generator, artifact, and runtime convention
unambiguous.

## Numerical grid and integration

The artifact schema is `nsb-moonlight-mie-phase-v1`. It contains 36 wavelengths
from 300 through 650 nm at 10 nm spacing and 181 scattering angles from 0 through
180 degrees at 1 degree spacing. Rows are wavelength-major and retain the compact
text format understood by `ScatterGrid`; runtime evaluation remains bilinear LUT
interpolation.

Each mode is integrated in ln(radius) with composite Simpson quadrature over
plus/minus five lognormal standard deviations and 800 even intervals. All axes,
bounds, formatting, ordering, and solver inputs are pinned in
`crates/nsb-data-tools/config/moonlight-aerosol-nsb-v1.toml`.

### Convergence evidence

The ignored `production_quadrature_convergence` test records expensive sensitivity
runs. Relative to production:

| Perturbation | maximum pointwise phase change | maximum absolute change in g |
|---|---:|---:|
| 400 instead of 800 radius intervals | 1.107% | 9.84e-5 |
| four instead of five radius sigmas | 22.65% | 1.37e-3 |
| six instead of five radius sigmas | 5.78% | 7.49e-5 |

The bound sensitivity is concentrated in the very narrow forward peak produced
by the mathematically unbounded coarse-mode lognormal tail; integral diagnostics
are stable. Five sigma is retained as the smallest practical, explicitly bounded
interpretation of a distribution whose physical large-particle cutoff is not
published. This forward-angle limitation is part of the v1 model uncertainty.

A deterministic coarsening study found that reconstructing the 10 nm rows from
20 nm samples gives 0.022% median, 0.256% 99th-percentile, and 0.520% maximum
pointwise differences. Reconstructing the 1-degree grid from 2-degree samples is
not adequate near the forward peak (up to 231% at 1 degree); away from angles
below 5 degrees its median, 99th-percentile, and maximum differences are 0.077%,
1.61%, and 2.97%. These results justify 10 nm and retaining the 1-degree grid.

## Physical and integral validation

The lifecycle validation checks finite/non-negative values, strictly increasing
axes, exact domain endpoints, the 4-pi integral, a physical asymmetry range, and
forward scattering at every wavelength. For the admitted artifact:

- worst relative 4-pi trapezoidal normalization error: `2.291e-11`;
- asymmetry-factor range: `0.573151` to `0.675545`;
- `P(0 deg) > P(90 deg)` at every wavelength;
- output SHA-256: `b74ee3c8e1039cdc0cc323bfa488c04cb2d09ce3871932fa957358c677d7e43d`.

Jones Fig. 9b provides only a graphical primary-reference comparison. The NSB
result reproduces the reported strong forward lobe, much weaker intermediate
and backward scattering, and modest wavelength dependence. No quantitative
curve values are published, so exact numerical agreement is not claimed.

## Historical ESO diagnostic only

After the model and production settings were fixed independently, the generated
grid was compared offline with common points in the historical LUT. Across 1,448
common wavelength/angle points, the median NSB/legacy ratio is 1.221 and median
absolute relative difference is 27.34%. The largest relative difference is at
650 nm and 0 degrees: ratio 4.278 (`106.034` versus `24.786`). Representative
NSB/legacy ratios at 300 nm for 0, 10, 30, 90, 150, and 180 degrees are 2.540,
0.671, 1.061, 1.298, 0.767, and 0.638. At 650 nm they are 4.278, 0.823, 0.900,
1.326, 1.672, and 1.303.

The discrepancy is expected because the historical solver details, radius
cutoffs, refractive-index treatment, normalization processing, and exact bytes'
provenance are not fully documented. No generator input was changed in response.

## Jones 2013 end-to-end impact

Using the same `main` solar spectrum, optical-depth code, empirical
`JONES_MIE_WEIGHT`, and historical multiple-scattering table, the isolated Mie
replacement changes the three regression geometries as follows:

| Separation | integrated 300--650 nm | B diagnostic | V diagnostic |
|---:|---:|---:|---:|
| 97.523 deg | +2.767% | +3.609% | +6.898% |
| 4.000 deg | -23.609% | -23.546% | -19.049% |
| 52.216 deg | +5.741% | +6.435% | +3.048% |

The large close-Moon change follows directly from the independently calculated
forward lobe and is intentionally not hidden by retuning `JONES_MIE_WEIGHT`.
Regression pins were refreshed only after this impact was measured.

## Transition policy and limitations

This PR uses transition option A: activate the distributable phase grid now and
retain `sscatcor_m15s1.dat` temporarily. The correction is only a few-percent
higher-order multiplier in the Jones reference model, so it remains usable as a
planning approximation, but the mixed pair is not claimed to be a coherent new
radiative-transfer calibration. Issue #217 must consume the versioned aerosol
configuration and replace that table before the v0.1.0 redistribution gate can
be fully cleared.

Spherical particles, constant real refractive index, the five-sigma cutoff, and
the paper's underspecified log-width notation are known limitations. Jones also
notes that Mie spheres can underrepresent nonspherical large-angle backscatter.

## Reproduction

Run, in order:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering update --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering build --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering validate --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering publish --config crates/nsb-data-tools/config/moonlight-scattering.toml
```

The pinned model-input SHA-256 is
`6fa37780c56e8f5ade0a2083780aa228570be786e1d4812b30468ec27f355afd`.
Running build twice produces the identical output checksum above.

## References

- Jones et al. 2013, A&A 560 A91, DOI 10.1051/0004-6361/201322433.
- Patat et al. 2011, A&A 527 A91, Cerro Paranal extinction curve.
- Warneck & Williams 2012, *The Atmospheric Chemist's Companion*.
- Bohren & Huffman 1983, *Absorption and Scattering of Light by Small Particles*.
