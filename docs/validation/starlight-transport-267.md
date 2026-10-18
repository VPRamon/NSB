# Spectral Starlight transport validation (#267)

Initial main: `dd59f5a7fb13d8edb98be33590a8cbe25b8bd61f` (merged #266).
No overlapping open implementation PR existed at the initial audit. #259/#260's
spectral product/API functionality already exists; no replacement was added.
Siderust remains pinned to `83d1ebb29ca1ae0b327d5784d13eda231d52c120`;
release blocker #265 remains open. Abandoned #264 is not a dependency.

## Synthetic validation

Offline unit tests construct checksum-validated complete RING products for one,
three, seven and fourteen bins, including nonuniform widths. They exercise typed
bin-integral/density conversion, vacuum recovery, exact-view pixel selection,
seam/poles/horizon, three representative sites and two epochs, unsupported
NESTED ordering, finite geometry, unsupported/partial/missing/union bins, invalid
source/schema/checksum/origin/radiance, legacy behavior, source provenance,
unknown uncertainty, absorption, Rayleigh, aerosol SSA endpoints, HG orientation
through the authoritative operator, homogeneous/eight-layer composition,
repeatability, independent wavelength refinement and explicit convergence/work
failures. Existing #190 analytical/SciPy/phase/path/angular tests are unchanged.

For uniform illumination and wavelength-independent literal optics, the bin
result equals `width * single_scattering(midpoint)`; this equality is valid only
for that deliberately wavelength-independent synthetic case. Pure absorption
matches `I_bin * exp(-tau/mu)` and once-scattered radiance is exactly zero.
The wavelength-dependent profile test spans 360–570 nm and independently refines
wavelengths; stringent unresolved spectral tolerance fails separately from
angular/path convergence. The anisotropic NSIDE 1/2/4 HG g=0.85 fixture records
angular convergence failures without changing the reference tolerances.

## Frozen-pilot boundary and reproducibility

Pinned independent evidence:
[nsb-validation 71bfce5 report](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/reports/starlight-empirical-validation-final/report.md),
[scattering producer](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/scripts/scatter_xp_pilot.py),
[protocol](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/configs/xp-scattering-pilot-v1.json),
[boundary projection](https://github.com/VPRamon/nsb-validation/blob/71bfce532f1911b795fa7658bde1e0edbc1cf775/src/nsb_validation/starlight/disort_boundary.py).

The original local 83,561,301-byte map was read without modification and its
SHA-256 verified as
`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
It is Galactic RING NSIDE128, seven integrated 30 nm bins from 360–570 nm.

Recovered reference conditions: DESI exposure 129304/b0; UTC start
2022-04-10 11:02:17.169, duration 1172.251 s, mid-exposure geometry; longitude
−111.59989°, latitude +31.96403°, height 2097 m; geometric fiber-zero altitude
76.9256911°, north-clockwise azimuth 271.8975544°, refraction disabled.
Pressure 779.5 hPa is a declared standard-atmosphere scenario, not a measured
exposure pressure. Aerosol tau550=0.03, Angstrom exponent=1.38, SSA=1,
HG g=0.65, zero molecular absorption; constant optical mixture with height,
black ground. Normalized Rayleigh phase has beta2=0.1; HG moments are g^l.
The historical Rayleigh column for wavelength `u` in micrometers is
`(779.5/1013.25)*0.008569*u^-4*(1+0.0113*u^-2+0.00013*u^-4)`.
This differs from Siderust's Bodhaine prescription; production coefficients
were not fitted or changed to match it.

The original nsb2 pin is `bc9320db03fbff69997ce366c6e76a37339c5d00`.
It sums above-horizon HEALPix illumination with bin-center kernels. The
independent scalar PythonicDISORT 1.8 reference instead conservatively projects
pixel contributions into GL cells and cosine angular moments centered on the
view, then extracts a first-order limit. Its full scattered reference includes
higher orders. The case-specific integrated values, photons/(cm² ns sr), are:

| Historical computation | Value |
| --- | ---: |
| nsb2 first order | 0.0025991471011773495 |
| Independent first order | 0.0025990756849478282 |
| Independent multiple scattering | 0.0033296021852152395 |

**Exact replay is not claimed.** The optional NSB example uses full-bin GL
integration and the actual Siderust Bodhaine profile, with a one-layer
homogeneous mixture. It is a diagnostic, not the historical compatibility
fixture. The geometry epochs in its documented invocation were calculated with
Astropy 8.0.2 and bundled offline IERS data: TT JD2459679.9675055384 and
UT1 JD2459679.966703672. They are explicitly supplied, rather than hidden
UTC=UT1 assumptions; historical IERS-version identity is not claimed matched.
Exact replay needs a scientifically matched historical-column operation plus a
conservative HEALPix angular boundary integration within the work cap (#192).
No coefficient tuning, 28.10% multiplier, new multiple-scattering solver,
observational accuracy score or empirical admission was introduced.


## Bounded execution measurements

Release measurements on Linux x86_64 / Intel Core Ultra 9 185H, on a shared
workstation. Timings are diagnostic, with no machine-dependent assertions.
The eight-layer NSIDE4 case transports three native bins with literal
wavelength-independent Rayleigh tau=0.1, aerosol tau=0.03, SSA=1 and HG g=0.65.
For each bin the configured whole spectral plan is 127,871,856 work units;
uniform-sky acceptance uses wavelength order 4 and scalar orders 32/64/32.
Across 21 executions the three-bin median is **106.709 ms** for uniform sky
(success), **126.759 ms** for anisotropic sky (explicit nonconvergence).

The spectral-profile synthetic NSIDE1 example accepts all three 30 nm bins at
wavelength order 4, scalar orders 32/64/32, with 25,456,368 conservative work
units per configured bin plan. First-bin TOA/direct/single are
0.003 / 0.0020267006978094445 / 0.0003240604633770209 in integrated photon units.
The wavelength-order changes are 3.72e-10 direct and 2.30e-9 single; the
accepted relative wavelength target is 1e-5, independently of scalar 1e-4.
Representative per-bin times were 11.9–16.6 ms.

The frozen-map diagnostic first fails all seven bins at scalar orders 64/128/64
(9.7–10.7 ms per failed bin). The denser documented invocation uses initial
16/32/2 with four refinements, reaching **256/512/32**, while its whole configured
spectral plan is **212,147,684 work units** per bin, below 250M. Every bin still
fails angular/path acceptance (131–206 ms each); the example exits nonzero and
returns no partial pilot total. This demonstrates an unresolved real-map
integration problem within these bounded plans, not equivalence to the
historical optical law. A fifth doubling of this plan exceeds the budget.
The scalar kernel's 8 MiB quadrature-buffer guard remains active at every node.
No general LUT/cache/resampling/parallel integration framework was added.

Reproduce the layered timing diagnostic with:

```sh
cargo test -p nsb --release --lib representative_layered_cost_diagnostic \
  --locked -- --nocapture
```
