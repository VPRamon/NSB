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
The generated rows use the analytic coefficient normalization; validation
independently checks the resulting grid with a piecewise-linear solid-angle
integral, making generator, artifact, and runtime convention unambiguous.

## Numerical grid and integration

The artifact schema is `nsb-moonlight-mie-phase-v1`. It contains 36 wavelengths
from 300 through 650 nm at 10 nm spacing and 355 scattering angles from 0 through
180 degrees on a refined, non-uniform grid. The grid uses 0.0125-degree spacing
at the forward edge, progressively coarsening to 1 degree between 10 and 170
degrees, then refining symmetrically to 0.125 degrees at the backward edge.
Rows are wavelength-major and retain the compact text format understood by
`ScatterGrid`; runtime evaluation remains bilinear LUT interpolation.

Each mode is integrated in ln(radius) with composite Simpson quadrature over
plus/minus eight lognormal standard deviations and 1280 even intervals. All
axes, bounds, formatting, ordering, and solver inputs are pinned in
`crates/nsb-data-tools/config/moonlight-aerosol-nsb-v1.toml`.

### Convergence evidence

The ignored `production_quadrature_convergence` test records expensive sensitivity
runs. Relative to the admitted ±8σ/1280 production configuration:

| Perturbation | maximum pointwise phase change | maximum absolute change in g |
|---|---:|---:|
| seven instead of eight radius sigmas, 1120 intervals | 0.019768% | 1.184209e-9 |

The bound sensitivity is concentrated in the very narrow forward peak produced
by the mathematically unbounded coarse-mode lognormal tail; the asymmetry
integral is stable. Eight sigma is retained as the explicit production bound
because the physical large-particle cutoff is not published. This forward-angle
limitation is part of the v1 model uncertainty.

The lifecycle's direct-solver interpolation probes report a worst relative
error of `1.781e-3` for the admitted refined angular grid. This check includes
sub-cell probes at the forward and backward edges and remains independent of
the artifact interpolation implementation.

## Physical and integral validation

The lifecycle validation checks finite/non-negative values, strictly increasing
axes, exact domain endpoints, the 4-pi integral, a physical asymmetry range, and
forward scattering at every wavelength. For the admitted artifact:

- worst relative 4-pi trapezoidal normalization error: `7.371e-5`;
- asymmetry-factor range: `0.573705` to `0.675937`;
- worst absolute coefficient-vs-grid asymmetry difference: `1.513e-5`;
- worst direct-solver interpolation probe error: `1.781e-3`;
- `P(0 deg) > P(90 deg)` at every wavelength;
- output SHA-256: `8ac2548e2699dee1448f60d867d4c2fd5a49b4702dba63297972e81cb3cb4bbc`.

Jones Fig. 9b provides only a graphical primary-reference comparison. The NSB
result reproduces the reported strong forward lobe, much weaker intermediate
and backward scattering, and modest wavelength dependence. No quantitative
curve values are published, so exact numerical agreement is not claimed.

## Historical ESO diagnostic only

After the model and production settings were fixed independently, the generated
grid was compared offline with the historical LUT where the domains overlap.
That comparison is diagnostic only: the historical solver details, radius
cutoffs, refractive-index treatment, normalization processing, and exact bytes'
provenance are not fully documented. The historical `mie_m15s1.dat` bytes are
not a generator input and are not shipped by NSB.

The discrepancy is expected because the historical solver details, radius
cutoffs, refractive-index treatment, normalization processing, and exact bytes'
provenance are not fully documented. No generator input was changed in response.

## Jones 2013 end-to-end impact

Using the same `main` solar spectrum, optical-depth code, empirical
`JONES_MIE_WEIGHT`, and historical multiple-scattering table, the admitted
artifact produces these regression outputs:

| Separation | integrated 300--650 nm | B diagnostic | V diagnostic |
|---:|---:|---:|---:|
| 97.523 deg | 0.09807135342489255 | 81.15667280395974 | 23.99613366393292 |
| 4.000 deg | 0.25448852391269283 | 263.7537714985368 | 111.0051406728119 |
| 52.216 deg | 0.08025208520210352 | 72.16654291788724 | 23.77908352554552 |

These values are pinned by the runtime regression test; no retuning of
`JONES_MIE_WEIGHT` is performed.

## Transition policy and limitations

This PR uses transition option A: activate the distributable phase grid now and
retain `sscatcor_m15s1.dat` temporarily. The correction is only a few-percent
higher-order multiplier in the Jones reference model, so it remains usable as a
planning approximation, but the mixed pair is not claimed to be a coherent new
radiative-transfer calibration. Issue #217 must consume the versioned aerosol
configuration and replace that table before the v0.1.0 redistribution gate can
be fully cleared.

Spherical particles, constant real refractive index, the eight-sigma cutoff, and
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
`d63543d5b168e27669479fc0004f0a9c21f94de81b83920981e4a61c8ae24e82`.
Running build twice produces the identical output checksum above.

## References

- Jones et al. 2013, A&A 560 A91, DOI 10.1051/0004-6361/201322433.
- Patat et al. 2011, A&A 527 A91, Cerro Paranal extinction curve.
- Warneck & Williams 2012, *The Atmospheric Chemist's Companion*.
- Bohren & Huffman 1983, *Absorption and Scattering of Light by Small Particles*.
