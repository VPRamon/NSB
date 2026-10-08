# NSB multiple-scattering correction v1

## 1. Scientific objective

This report admits `moonlight_multiscatter_nsb_v1.dat`, an NSB-owned compact
wavelength × Moon-target-separation correction for the Jones et al. (2013)
spectral moonlight path. Production generation neither reads nor fits the
ESO `sscatcor_m15s1.dat` reference table. Jones et al. (2013), A&A 560 A91,
DOI `10.1051/0004-6361/201322433`, is the primary scientific reference.

## 2. Published multiple-scattering definition

Jones treats the Moon as a point source. Its single-scattered radiance is the
line-of-sight integral (their Eq. 6)

\[
I_{SS}(A_0,z_0)=\int_0^{s_2}\frac{C_{scat}}{4\pi}n(S)P(\theta)
F_\mathrm{Moon}\exp[-\tau(S)]\,ds.
\]

`P` is normalized so its solid-angle integral is `4*pi`. Double scattering is
the sum of two such events (molecule, aerosol, or ground in the published
calculation). Jones calls scattering orders greater than two `I_MS` and assumes
that every consecutive-order ratio equals `r = I_DS/I_SS`. Their Eq. 10 is

\[
I_{MS}=I_{DS}\left[(1-r)^{-1}-1\right],\qquad r\le 0.9.
\]

Therefore the runtime quantity is multiplicative, not additive:

\[
f=\frac{I_{SS}+I_{DS}+I_{MS}}{I_{SS}}
  =\frac{1}{1-\min(I_{DS}/I_{SS},0.9)}.
\]

It is dimensionless. The wavelength dependence enters through molecular and
aerosol optical depth and phase functions; separation enters through the two
scattering directions. The paper reduces target zenith, Moon zenith, and
relative azimuth by weighted averaging. It reports about 5% average error for
the reduced double-scattering representation and says higher orders are usually
smallest at low optical depth. The exact Patat-observation and theoretical Moon
position weights are not published numerically.

## 3. Facts, inherited assumptions, and NSB choices

| Category | Admitted statement |
| --- | --- |
| Explicitly published | Eq. 6 single scattering; two-event double scattering; geometric higher-order series; `r <= 0.9`; multiplicative `f`; Rayleigh phase `3/4(1+cos²θ)`; 744 hPa, 2.64 km, 7.99 km molecular scale height, 1.2 km aerosol scale height, aerosol scattering/extinction ratio 0.97 |
| Jones/Paranal atmosphere | Liou/Jones Rayleigh optical-depth law; Patat aerosol extinction `0.014 lambda^-1.38 mag/airmass`; remote-continental aerosol mixture and Mie phase function shared with issue #216 |
| NSB design choice | Scalar, plane-parallel, forced-collision Monte Carlo; no ground reflection; exponential vertical profiles above the observer; two equally weighted symmetric geometries; 140° boundary value at 180°; fixed-seed deterministic rounding |
| Not publicly specified | Exact reference zenith/azimuth weights, numerical double-scattering quadrature, and original table-reduction details |

No unknown was inferred by optimizing against the reference table.

## 4. Atmosphere model

Rayleigh and aerosol columns use different exponential scale heights. At height
`h` above the observer their remaining vertical columns are
`tau_R exp(-h/7.99 km)` and `tau_A exp(-h/1.2 km)`. The local collision species
is sampled from the corresponding extinction gradients. Aerosol scattering has
single-scattering albedo 0.97. Molecular line absorption and ground reflection
are omitted for the higher-order correction, consistent with the paper's
statement that its effective molecular absorption was not applied to higher
orders. Omitting ground reflection is an NSB simplification necessitated by the
absence of an unambiguous redistributable Paranal surface spectrum.

## 5. Relation to issue #216

The calculation consumes `nsb-moonlight-mie-phase-v1`, SHA-256
`8ac2548e2699dee1448f60d867d4c2fd5a49b4702dba63297972e81cb3cb4bbc`,
generated from `nsb-moonlight-aerosol-model-v1`, SHA-256
`d63543d5b168e27669479fc0004f0a9c21f94de81b83920981e4a61c8ae24e82`.
Finalization regenerates the #216 Mie bytes from that aerosol configuration and
requires a byte-identical hash. Partition provenance and the runtime manifest
repeat both identities. A mismatched pair fails before publication.

## 6. Solver and licence

The in-tree solver is `nsb-forced-collision-plane-parallel-mc-v1`, licensed
AGPL-3.0-only with NSB. It uses no external executable or data library. Each
photon is forced to collide before its current top/bottom boundary; the packet
weight is multiplied by that collision probability. A next-event estimator
records radiance toward the observer after collision one (`I_SS`) and collision
two (`I_DS`). Scattering directions are drawn from the Rayleigh phase function
or the #216 tabulated Mie CDF. This is an independent implementation, not the
Jones code and not libRadtran.

## 7. Numerical method

For each cell, 32,768 histories are run in each of four deterministic
replicates at each of two geometry samples. The estimator traces exactly two
orders because Jones defines all subsequent orders algebraically from their
ratio. The reduction averages replicate/geometry ratios, applies the published
0.9 cap, and serializes eight decimal places. Fixed seeds and a fixed reduction
order make raw partitions and the admitted LUT bit-for-bit reproducible.

## 8. Geometry parameterization

For each separation, source and target have equal zenith distance. Two samples
use zenith floors 30° and 45°; each rises to at least half the requested
separation and is capped at 80°. Relative azimuth is solved exactly from the
spherical cosine relation. These samples avoid silently importing the
unpublished Jones weights while spanning moderate and longer slant paths.

Exact 180° separation forces both rays onto the plane-parallel horizon. The
180° runtime node therefore repeats an independently computed 140° boundary
solution. The model is most defensible through 140°; 140–180° is a documented
constant boundary approximation.

## 9. Marginalization and reduction

The compact value is the equal-weight mean of the two geometry-specific
`I_DS/I_SS` ratios and four replicates. This is an explicit NSB marginalization,
not a claimed reconstruction of the unavailable Patat/Moon-position weights.

## 10. Production grid

The LUT uses 36 wavelengths from 300 to 650 nm in 10 nm steps and separations
`0,10,...,140,180` degrees. This aligns wavelength nodes with the shared Mie
product and retains the angular coverage required by the runtime. Bilinear
interpolation is deterministic and clamps only outside these declared axes.

## 11. Local partition execution

One wavelength is one restartable partition (`w0300` through `w0650`). The
local executor processes these partitions deterministically. Partitions exist
for reproducibility, provenance, and resumability rather than cluster
distribution. The complete production calculation is lightweight enough for a
normal local machine.

## 12. Deterministic seed policy

The stable identifier
`namespace:wavelength:separation:geometry-index:replicate-index` is hashed with
FNV-1a-64 and expanded with SplitMix64. Scheduler IDs, process IDs, clocks, and
execution order never enter a seed.

## 13. Convergence study

The preregistered photon-count check compares 8,192 and 32,768 histories per
replicate at 300/500/650 nm and 0°/90°/140°. The acceptance threshold is 3%
relative change in the final correction. The worst observed change is 1.1147%
(300 nm, 90°). Other changes range from 0.0007% to 0.6637%. The admitted sweep's
largest replicate standard error in `I_DS/I_SS` is 0.008010. Final-grid
continuity also passes: its largest adjacent angular-node jump is 0.9281, in the
strong blue multiple-scattering regime, with no NaN, Inf, negative, or
cap-exceeding value.

## 14. Single-scattering validation

The implementation has unit coverage for Rayleigh normalization and the #216
validation independently verifies Mie `4*pi` normalization (worst trapezoid
error `7.371e-5`), asymmetry, and direct-solver interpolation (`1.781e-3`
worst relative error). There is not yet an independent analytic end-to-end
test of the first-order Monte Carlo estimator itself. Forced-collision
sampling avoids a rare-event bias in the single-scattering denominator, but
that implementation property is not a substitute for the missing reference
comparison.

## 15. Double/multiple-scattering validation

`I_SS` and `I_DS` are accumulated separately, so the ratio is directly
inspectable in every partition. Physical-range, statistical-precision,
continuity, provenance, and complete-partition gates are machine checked. Unit
tests pin the Jones series/cap, geometry, seeds, deterministic serialization,
and missing-partition failure.

## 16. Independent RT comparison

No second solver is claimed. libRadtran/uvspec is not installed in the admitted
environment, and the paper does not publish its individual comparison cases.
Jones reports that 75% of cases with zenith angles below 70° agreed with
libRadtran within 10%, but that statement is qualitative context rather than a
numeric validation of this LUT. A future spherical/DOM cross-check is warranted,
especially beyond 140°. The order-resolved in-tree implementation and the
independent #216 Mie solver do not share a transport kernel.

## 17. Primary-reference comparison

The mathematical definition, ratio cap, optical-depth laws, vertical scale
heights, phase normalization, aerosol albedo, and stated omission of
higher-order molecular absorption are exact textual/equation reproductions.
The paper provides no numeric correction cells. Its qualitative expectation of
larger multiple scattering at larger optical depth is reproduced: factors are
largest at 300 nm and approach unity toward 650 nm.

## 18. ESO LUT comparison (diagnostic only)

This comparison was run only after the model/config and production artifact
were fixed. On common 300–650 nm and separation nodes, median absolute factor
difference is 0.2375, median absolute relative difference 14.71%, maximum
absolute difference 2.7628, and maximum absolute relative difference 45.35%.
The NSB grid is generally lower: median signed differences range from -28.01%
at 300 nm to -12.20% at 500 nm and -18.38% at 650 nm. Median signed angular
differences range from -3.03% at 10° to about -17.45% at 70°.

Representative `(NSB, reference)` factors are: 300 nm `(1.1203,1.9360)` at
0°, `(5.0783,5.0900)` at 90°, `(3.3416,6.0920)` at 140°; 500 nm
`(1.0120,1.1040)`, `(1.2984,1.5590)`, `(1.2424,1.4050)`; and 650 nm
`(1.0035,1.0680)`, `(1.1288,1.4870)`, `(1.1187,1.3710)`. No parameter was
changed in response to these differences.

## 19. Jones end-to-end impact

Against the immediately pre-change runtime (same analytic solar path), three
representative `(integrated 300–650 nm, B, V)` cases change by:

- 97.523° separation, Moon/target zenith 36°/60°: `-11.44%, -12.48%, -13.31%`;
- 4° separation, 36°/40°: `-33.87%, -29.39%, -23.48%`;
- 52.216° separation, 62°/15°: `-13.61%, -8.17%, -9.86%`.

The largest change is the close-Moon case, where the new independently
generated correction is much lower at small separation. The direction is
consistent with the offline LUT comparison and is accepted as a model change,
not hidden by fixture adjustment. Regression tests pin the new absolute values.

## 20. Known limitations

The transport is scalar and plane-parallel; polarization, refraction, Earth
curvature, clouds, surface reflection, and line absorption are absent. Geometry
weights are generic rather than reconstructed Paranal observing frequencies.
The higher-order geometric series inherits the Jones approximation. The 180°
node is a boundary policy, not a physical horizon solution. The product is a
published-model reconstruction for planning, not a site-calibrated atmosphere.

## 21. Reproduction commands

```text
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering update --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering build --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering validate --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- dataset moonlight-scattering publish --config crates/nsb-data-tools/config/moonlight-scattering.toml
cargo test --release -p nsb-data-tools production_photon_count_convergence -- --ignored --nocapture
```

Missing/corrupt partitions, source/config hash changes, changed solver commit,
Mie incompatibility, or failed physics gates stop finalization/publication.

## 22. Exact identities and resource use

The machine-readable run record is
[`production-runs/multiscatter-nsb-v1.toml`](production-runs/multiscatter-nsb-v1.toml).
The output SHA-256 is
`7251dd540a7dd0a10eceda9bec12e5061a1bbc29332ac2f92b44e4684f3cc050`.
The admitted local run used 36 wavelength partitions, 380.07 CPU seconds,
380.44 wall seconds, 7.4 MiB peak RSS for the sweep (8.5 MiB for final Mie
verification), no GPU, and about 176 KiB of partition JSON. This measured
resource use is why cluster execution is intentionally out of scope.
