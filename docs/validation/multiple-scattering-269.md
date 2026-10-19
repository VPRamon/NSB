# PR 269 scientific and engineering evidence

Initial NSB revision: `9b75053b813b12507f37650bd6523779555201e6`.
Initial/main base: `f556125901b94a19577db062cc21dfb808aeba7f`.
[Existing PR 269](https://github.com/VPRamon/NSB/pull/269) remains unmerged.
The final pushed revision and required-check URLs are recorded in the PR;
this document describes the executed scientific evidence, not older CI greens.

Independent generators, exact outputs, warnings and checksum manifest:
[nsb-validation 08b50e604589702405267f6b1ebae521dfaa1b24](https://github.com/VPRamon/nsb-validation/tree/08b50e604589702405267f6b1ebae521dfaa1b24/scripts/multiple-scattering).
NSB stores immutable `disort-v1.json`, `orders-v1.json`, `spectral-v1.json`
and `spectral-optics-v1.json` in `crates/nsb/tests/data/multiple-scattering/`
so they are present in Cargo archives. `multiple-scattering-reference-provenance.json`
beside this report pins their independent generators and receipts. No reference generator is an NSB runtime dependency.

## Audit findings and corrections

1. The historical formatting and Copy/clone failures were reproduced locally.
   Formatting was applied and the unnecessary optics clone removed.
2. Both Python generators violated the existing dataset contract. They were
   moved to the validation repository; neither the guard nor its exceptions
   were changed. Their outputs were regenerated, not assumed. The matrix fixture
   was correct to floating-point rounding. The independent E1/Fredholm fixture
   was also correct. No production coefficient was changed to fit either.
3. The old tail threshold used peak internal radiance as a relative scale. That
   can accept excessive relative error in the requested ground observable.
   The threshold now uses ground single plus higher radiance. The rigorous
   ground-tail bound uses `g * ||I_n||inf / (1-q)`. A longer-sum regression
   verifies that the bound contains the actual discrete omitted contribution.
4. Raw phase-row normalization previously allowed a 10% departure. Active
   internal rows and each active requested-view row now must resolve unity to
   the smaller of the angular target and 1e-3. No renormalization conceals aliasing.
5. Finite-cell midpoint sources have genuine vertical bias. They are retained
   as a consistent approximation, explicitly documented and independently
   refined. They are not described as an exact first-order antiderivative.
6. Native-bin and conservative pixel-boundary integration were absent. A new
   opt-in path integrates fine HEALPix cell areas and kernels independently of
   the internal diffuse field, then integrates the complete original bin with
   independent wavelength refinement. Exact direct-view lookup is retained.
7. The original smooth example fails its diffuse-grid test in its very thin
   configuration. It now demonstrates a documented moderate mixed column and
   an explicit resolution plan that actually converges. Tolerances were not
   increased to conceal that failure.
8. Algebraic matrix agreement alone did not establish continuum physics.
   External DISORT and continuous-height order-specific references now supply
   independent evidence, including nonzero azimuth modes and stratification.

Upward/downward sweep signs, black boundaries, constituent extinction accounting,
order accumulation and the infinity-norm contraction argument were checked.
They are consistent with the specified discrete scalar RTE. Arbitrary-source
continuum accuracy is not established by this audit or by a small order tail.

## Matched independent stationary transfer

Python 3.12.13 / NumPy 2.5.4 / SciPy 1.18.1 / PythonicDISORT 1.8, offline,
`OPENBLAS_NUM_THREADS=1`. DISORT solves vertical eigenmodes and Fourier modes;
it does not share NSB's finite-cell sweeps. Its 32/64/128/256-stream results
are retained. A separate 256-mu / 512-azimuth layer-exact formal integral
isolates first order. The full stationary solution minus exact unscattered
and first-order terms gives higher orders. No delta scaling, beam, emission,
reflection or fitted parameters are used.

All depths, SSA, phase parameters and illumination are explicit in the JSON.
Column extinction depths span 0.09–0.35; effective SSA is below one. HG g ranges
0–0.4. The stratified case has two distinct layers. The azimuthal mixed case
adds a cosine azimuth mode to a zenith-dependent sky; it checks direction/phase
conventions beyond axisymmetric tests. Other skies include uniform and smooth
zenith anisotropy. The view cosine is 0.8. Units below are spectral
photons/(cm² ns sr nm).

These are **public API** results: initial mu=4, azimuth=16, cells/layer=4;
accepted mu=8, azimuth=32, cells/layer=8; order target 1e-8, maximum 24 orders
for one layer / 16 for two. Diffuse angular/vertical change targets are 5%,
independent from the 1% continuum regression allowance. The actual errors are
reported rather than identifying a 5% grid change with 5% physical accuracy.
Timing is from optimized test builds on a shared Linux workstation.

| Case | Rust single | Rust higher | Relative first error | Relative higher error | Measured runtime |
| --- | ---: | ---: | ---: | ---: | ---: |
| azimuthal_mixed | 0.00173121329764 | 0.000231176690772 | 0.02979% | 0.17080% | 65.647853ms |
| hg | 0.00166476597685 | 0.000150680360447 | 0.02129% | 0.10895% | 57.455688ms |
| mixed | 0.00166220105813 | 0.000229615350871 | 0.02961% | 0.17206% | 61.326784ms |
| rayleigh | 0.00112756773598 | 0.000161092456299 | 0.04896% | 0.34060% | 59.934866ms |
| rayleigh_uniform | 0.000886983348236 | 0.000103950010751 | 0.03746% | 0.16115% | 54.86868ms |
| stratified | 0.00169687335145 | 0.000270231977362 | 0.00726% | 0.02585% | 96.366334ms |
| thicker | 0.00271952985383 | 0.000886188665701 | 0.05938% | 0.19802% | 77.273381ms |

The public plans cost 168,652,800 work units (one layer) or 229,367,808
(two layers). Estimated phase/field workspace ceilings are 4.16 / 4.31 MiB,
respectively. Accepted iterations are 10–17. Reported ground tails range
3.8e-12–1.7e-11. Angular/vertical changes are diagnostic; the reference differences
are measured errors against an independently refined numerical solution.

A separate finer-height comparison (16 subcells, mu=8, azimuth=16) gives maximum
relative error 0.03760% for first order and 0.24332% for higher orders across
these seven cases. Its first order also agrees with the authoritative merged
single-scattering solver within 0.1%, including mixed, azimuthal and two-layer
cases. This is numerical equivalence under matched optics, not empirical sky
accuracy. At fixed reference physics, no historical Gaia enhancement is used.

The largest 128→256 DISORT higher-order change is about 4.5e-5 relative. A
low-rank Rayleigh eigenvalue roundoff warning is preserved in the validation
receipts; stream refinement is provided and the warning is not suppressed.
The reference error is much smaller than the 1% Rust regression allowance.

## Second and third orders independently

The second reference integrates analytic azimuth Legendre moments and uses
full-interval GL zenith nodes, separate upward/downward adaptive DOP853 IVPs,
and adaptive ground-ray integration. There are no finite vertical source cells.
DOP853 is SciPy's eighth-order method, with relative target 1e-10 and absolute
1e-14; ground integration targets absolute 1e-13. These are integration controls,
not certified global error bounds. Angular streams 64/128/256/512 are retained.

Rust's individual orders are isolated in tests by scaling scattering while
keeping total extinction fixed through Siderust optics, followed by quadratic
Richardson extraction at scales 0.01, 0.005 and 0.0025. This is a test of the
actual production recurrence; no multiplier or extraction enters production.
The independently solved orders give the following relative differences:

| Case | Second-order difference | Third-order difference |
| --- | ---: | ---: |
| HG absorbing | 0.02969% | 0.22762% |
| Mixed | 0.05124% | 0.21602% |
| Rayleigh absorbing | 0.19740% | 0.54385% |
| Rayleigh uniform | 0.03697% | 0.13077% |
| Thicker | 0.03266% | 0.08223% |

Last independent angular changes are below 0.056% for third order. The 1%
regression tolerance accounts for that remaining angular error and NSB's
finite-cell error; it is not a fitted physical uncertainty. Stratified and
azimuth-dependent full higher-order sums are independently checked by DISORT,
not by this homogeneous order-specific IVP reference.

## Separate resolution studies

For isotropic TOA 0.025, aerosol extinction 0.12, SSA 0.7, absorption 0.02,
mu_view=0.8, the E1/Fredholm reference gives first
0.00095110331503202555 and higher 0.00013426097214553828 at 256 height cells.
The dense discrete matrix gives first 0.000952045150627305 and higher
0.00013390123542604275 at its deliberately coarse four-cell/four-mu grid.
Only the E1 kernel establishes an independent continuum comparison.

Angular refinement at fixed 16 cells (8 azimuth nodes):

| Positive mu nodes | First | Higher |
| ---: | ---: | ---: |
| 2 | 0.000986206832061881 | 0.000117226484964876 |
| 4 | 0.000952743589689807 | 0.000133356153562702 |
| 8 | 0.000950884676619396 | 0.000134371671299479 |

Vertical refinement at fixed 8 mu and 8 azimuth nodes:

| Cells | First | Higher |
| ---: | ---: | ---: |
| 2 | 0.000946944107676279 | 0.000137116242994029 |
| 4 | 0.000949774914553355 | 0.000135132785939791 |
| 8 | 0.000950651689158670 | 0.000134531188868813 |
| 16 | 0.000950884676619396 | 0.000134371671299479 |

The final order tail is 1.59e-13; the reference difference is orders of magnitude
larger. This explicitly separates iterative residual from discretization error.
Angular and height limits approach different finite-grid limits; agreement with
one reference value at one coarse grid can result from cancellation.
A separate long-sum test verifies the omitted-order bound and stricter order
criteria increase the included orders.

Vacuum identity, absorption/zero SSA, dark sky with black boundaries, Rayleigh,
aerosol and mixed constituents, split-layer identity, supported tabulated and
double-HG phases, invalid values, strict-grid rejection, forward-peak rejection,
overflow and computational limits have explicit Rust tests. Pure-conservative
or very thick cases may be refused by the conservative contraction bound.

For conservative Rayleigh depth 0.08, signed photon-flux closure is 3.2862e-5,
8.7490e-6, 2.2232e-6 and 5.5808e-7 at 2/4/8/16 height cells (8 mu nodes).
The nearly quadratic decrease measures the midpoint discretization's energy
balance defect rather than concealing it. Vacuum and pure absorption close
within 1e-13. The formal constant-source absorption integral is separately
checked from optical depths 1e-12 to 200. Thin isotropic limits approach the
analytic linear first-order coefficient while higher/first tends to zero.

## Native-bin HEALPix integration

Fine equal-area boundary cells retain parent radiance. Every parent through
native NSIDE8 has exactly factor² children under factors 2 and 4, including poles
and seams. Isolated-parent tests also check full-sphere radiance conservation,
hemisphere measure, local direction signs and clipping of visible parent area.
RING is the supported upstream ordering. The exact product checksum, native
bins, Galactic frame, observer and explicit TT/UT1 remain in the result.

A nonzero uniform synthetic 360–390 nm bin (integral 0.75), Rayleigh depth 0.08,
absorption 0.08, and view cosine 0.8 accepts boundary NSIDE128 after independent
8→16→32→64→128 refinement, with two consecutive 1e-4 changes. Diffuse acceptance
is mu=8 / azimuth=8 / cells=4; wavelength order is 4. Results in integrated
photons/(cm² ns sr):

* single: 0.02658873913628157;
* higher: 0.0031324449896208877.

Compared with the matched continuous-height 256-stream DISORT solution, first
and higher errors are approximately 0.1155% and 0.6090%. This is a finite-cell
reference, not an exact layer-integral method. An isolated bright zenith parent
with isotropic aerosol scattering (extinction 0.16, SSA 0.5) also accepts
NSIDE128 and gives single 0.004948728281451938, higher 0.000633168648809647.
Its area-conservation test is quantitative; no separate continuum accuracy
percentage is claimed for that strongly anisotropic map.

The complete-bin plan is 1,610,948,224 work units under its distinct 4 billion
cap. Peak field/boundary estimate is about 9.27 MiB under the unchanged 16 MiB
ceiling, excluding immutable input assets, result snapshots and allocator
bookkeeping. The standalone release example measured 27.22 s/bin on a busy
shared workstation; optimized Rust tests measured about 14 s/bin. These are
observations, not hardware-independent performance guarantees.

The wavelength-specific test uses a wavelength-dependent Siderust profile in
pure absorption. It rejects inadequate 2→4 refinement at target 1e-12, then
accepts further doublings, agrees with an independent 64-node wavelength
integral within 1e-12 relative, and differs measurably from bin-center sampling.
No higher-order wavelength uncertainty is fabricated. All three nonzero/zero
components have separate integration acceptance, and no cross-bin covariance is
assumed. The full-bin test through a scattering column has constant optics;
wavelength-dependent higher-order integration is independently checked below.

### Independently integrated wavelength-dependent higher orders

A second full 360–390 nm test uses a Siderust profile with pressure 200 hPa,
observer 2097 m, top 3097 m, aerosol tau550=0.04, Angstrom exponent 1.38 and
SSA zero. Rayleigh still scatters and the aerosol absorbs; both depths vary
with wavelength. The independent generator receives only exported upstream
optics (checked against Siderust in Rust), reconstructs its own 64-node
wavelength rule with NumPy, and solves each wavelength with external DISORT.
There is no duplicate production optical prescription or shared radiance result.

The 256-stream / 64-wavelength reference gives integrated direct
0.6478098452775395, single 0.03368977549691968 and higher 0.005334997081052671.
Rust's 4→8-node native-bin result is direct 0.6478098452775397, single
0.03365999573464938, higher 0.005372220132620359. Direct agrees to rounding;
first and higher differences are approximately 0.0884% and 0.6977%, dominated
by finite source cells. The reference's 128→256 higher-order angular change
is below 7e-7 relative. A 2→4-node Rust attempt correctly rejected insufficient
wavelength convergence. Its tolerance was preserved; the accepted plan uses
more wavelength nodes and the distinct, bounded full-bin CPU budget.

## Frozen Gaia XP comparison and exact blocker

The source bytes were loaded without modification and checksum verified:
`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
Recovered conditions are documented in
[the merged first-order evidence](starlight-transport-267.md#frozen-pilot-boundary-and-reproducibility).
They include exposure 129304/b0, site -111.59989°/+31.96403°/2097 m,
TT JD2459679.9675055384, UT1 JD2459679.966703672, geometric view
76.92569110795976° / 271.8975544363482°, aerosol tau550=0.03,
Angstrom exponent 1.38, SSA=1, HG g=0.65, no absorption, black ground.

Historical reference values are single 0.0025990756849478282 and total scattered
0.0033296021852152395; historical nsb2 single is 0.0025991471011773495.
Higher is 0.0007305265002674113 relative to the independent first-order value.
The historical Rayleigh polynomial, bin-center kernels and apparent-place
Astropy transform differ from current Siderust optics, full-bin integration
and its geometric rotation. The merged first-order report quantifies those
separate effects: historical-law compatibility single 0.00259905749633912,
current center single 0.00241726783764973, current full-bin single
0.00241960698610553. These inherited results are labelled as such, not newly
measured multiple-scattering outputs.

Actual new reproduction command (offline, after compilation):

```sh
cargo run --release -p nsb --example starlight_multiple_scattering --locked -- \
  /path/to/starlight_360_570_candidate.json \
  2459679.9675055384 2459679.966703672
```

The checked product loads, then preflight returns:

```
HEALPix multiple-scattering memory exceeds 16 MiB
```

The rejection was measured at 1.8 microseconds after loading. At boundary
NSIDE256 alone, the conservative allocated boundary capacity would be
786,432 directions × 48 bytes = 37,748,736 bytes, before phase/field buffers.
This implementation therefore cannot admit the native NSIDE128 pilot's required
128→256→512 boundary plan. Its HG/conservative diffuse convergence must also be
established before a successful physical comparison. No multiple-scattering
Gaia number, forced 28.1% factor or observational-accuracy claim is returned.
This is an exact reproducible **implementation limit**, not absence of a
continuous physical solution or unavailable source data.

## Compatibility and verification

The solver and native-bin adapter remain explicitly experimental. Existing
Rust source types and evaluator defaults, CLI, Python bindings, frozen assets,
calibration and holdouts are unchanged. The public API snapshot adds one opt-in
function and its typed native-bin result. Existing QTTY, Siderust optics and
astrometry, Optica phases, native-bin Starlight machinery and provenance are
reused. No new dependencies or crates are introduced.

Local validation includes formatting, warnings-denied all-feature Clippy,
workspace all-target/all-feature tests, Rust 1.89 all-target/all-feature check,
Rustdoc with warnings denied, doctests, release examples, installed abi3 wheel
and all 115 Python tests, dataset/scientific-asset contracts and the established
LLVM coverage system. Exact final coverage and final-SHA hosted CI evidence are
recorded with the PR; an older green SHA is not treated as final evidence.
The host required `PYO3_PYTHON` pointing at Python 3.12 for linking all-feature
Rust tests because its default Python 3.14 installation lacked the development
linker library. No repository workaround was added.

Issue #265 remains open. Current registry/release lookup still reports
Siderust 0.14.1, which lacks the required APIs. The established Git pin,
package identity and negative registry-publication guard are retained.

Remaining scientific limits: scalar plane-parallel geometry, black surface,
finite midpoint source cells, limited angular resolution and conservative
order norm bound; no general arbitrary-table/high-g/near-conservative guarantee;
no full
Gaia multiple-scattering admission or empirical accuracy/uncertainty estimate.
The restricted synthetic domain is validated; broader claims require evidence.

Refs #187, #192, #263, #265.
