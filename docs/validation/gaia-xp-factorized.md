# PR #270: full Gaia XP factorized multiple-scattering validation

Starting PR head: `a83441596404a08e15fc3925348d45a8ed13f496`. This follow-up
continues the same PR, based on the already merged #269. Final SHA and Actions
URLs are recorded in PR #270; this report avoids a self-referential source SHA.

The frozen seven-bin NSIDE128 product is now evaluated successfully with an
opt-in Fourier/positive-kernel projection method. The source is unchanged:
`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
This is numerical validation of a specified scalar transport experiment, not
observational admission or a claim of complete stellar-population accuracy.

## Mathematical decision and ownership

The #269 scalar RTE, Siderust coefficients/column, Optica phases, black ground,
up/down formal sweeps and physical successive orders remain authoritative.
Direct illumination still uses its original exact view lookup and transmission.
The new code replaces expensive angular operations within that solver.
No second optics implementation, map API, crate or production default is added.

A full spherical-harmonic expansion was considered. HG has moments g^l and
Rayleigh has only l=0,2 (p2=.1). The addition theorem factorizes the convolution,
but an order 32 full basis has 1089 coefficients per height. Streaming those
height-dependent moments is still expensive, and truncation requires a separate
positive-kernel/tail contract. It is a possible alternative, not a prerequisite.
A general per-view adjoint/LUT framework would add broader cache dependencies
and obscure full-field flux diagnostics. Neither is needed here.

Instead, at fixed incoming/outgoing mu the sampled rotational phase kernel is
circulant in azimuth. Exact finite DFT convolution replaces its dense pairwise
matrix. Every sampled Fourier mode is retained; an even real kernel stores its
conjugate spectrum without losing modes. Small grids retain the original dense
path. Rayleigh's finite azimuthal modes 0,1,2 are exact, not an HG approximation.
The transform is safe Rust, radix2 with bounded direct DFT for previously
supported non-power-of-two grids. No FFT crate or global state is introduced.
[PythonicDISORT's own Fourier formulation](https://pythonic-disort.readthedocs.io/en/latest/Pythonic-DISORT.html)
and the [spherical-harmonic addition theorem](https://dlmf.nist.gov/14.30.E9)
supply mathematical context; NSB still uses its own original height sweeps.

For the external boundary, nonnegative hat functions h_a partition unity:

`sum_b I_parent(b) dOmega_b K(b) ~= sum_a [sum_b I_parent(b) dOmega_b h_a(b)] K(a)`.

This interpolates the *transport kernel*, not photometry. Each refined HEALPix
sample retains its exact native parent mean. Source masses and their first mu
moment are conserved to summation roundoff. Cosine-grid nodes cluster at the
horizon and zenith (`mu=sin²(pi t/2)`); azimuth wraps periodically. Independent
mu-only and azimuth-only refinements test the approximation. Incoming attenuation
is applied at each wavelength/height after geometric mass construction.
Fourier convolution avoids projecting each source pixel onto each diffuse ray.
The view is evaluated at its actual azimuth; no view snapping or source smoothing.

The positive sampled operator is not silently renormalized. A Fourier source
negative beyond relative 1e-12 roundoff returns an error; only transform-roundoff
negatives are floored at zero. Raw active phase rows and requested-view rows
still must resolve unity to min(angular_target,.001). Kernel refinement is
additional to that audit. No delta scaling or empirical enhancement is used.

## Contraction and physical orders

The old bound based on the most grazing ray can be arbitrarily close to one.
For the same positive discrete field operator K, construct
`w=sum_{r=0}^{p-1} K^r 1`, p<=8. Then `K w=w-1+K^p 1`. A componentwise bound
`K w <= q w`, q<1, establishes a weighted sup-norm contraction. The smallest
resolved q is retained (with 1e-12 numerical margin); failure remains explicit.
The view gain is evaluated in the same weight, giving the omitted-view tail
`gain * ||I_n/w||inf / (1-q)`. This strengthens a valid bound, not a stopping
threshold. The original unweighted bound remains for well-conditioned cases.
Six field planes account for construction/replacement transients.

Orders still arise solely from repeated scattering, transport and extinction.
First is not direct light; second/higher are not a multiplier. The finite-cell
first order is compared with the independently layer-exact #266/#268 reference.
No correction to a production atmosphere is made to fit the historical pilot.

## Work and memory contract

Let B be boundary cells, Q diffuse mu directions, P_d diffuse azimuths, U kernel
mu nodes, P_b kernel azimuths, K height cells and N physical orders. Dense
internal O(N K (Q P_d)^2) work becomes
O(N K [Q² P_d + Q P_d log P_d]). Boundary pair projection O(B K Q P_d) becomes
O(B + K Q U P_b), plus bounded transforms and phase-kernel construction.
Storage is independent of B: compressed phase matrices, geometric mass banks,
phase kernels and fields. The full input map is immutable and outside workspace.

Masses are evaluation-local and reused between wavelength nodes. Analytic
Optica HG/double-HG phases are explicitly wavelength independent, so their
kernels are reused within that bin/evaluation. Phase parameters, kernel grids,
diffuse mu nodes and view mu are fixed by that invocation. Source/site/epoch
are used when constructing that invocation's mass bank. No source, geometry,
optical depth, SSA, absorption or attenuation is reused across evaluations.
Spectral tables bypass phase reuse; a wavelength-varying table regression checks
this. There is no public cache or broad cache/LUT framework.

The streamed reference keeps its four-billion full-bin /250-million scalar
limits. The opt-in projected method has a separately derived 32 billion full-bin
and one billion four-grid diffuse-work contract, with the same 16 MiB workspace
ceiling. Units charge transform butterflies, complex multiply/accumulation,
phase construction, geometry, height sweeps, all orders and all refinements;
they are not CPU instructions and are not directly comparable to the old
pair-kernel unit count. Its actual accepted full-bin plan is 27,170,875,264 units.

The two kernel-axis diagnostics are charged once per wavelength *after*
boundary acceptance, as implemented. All possible boundary levels and requested
wavelength nodes are still charged, even on early exit. Analytic phase kernels
are charged once; spectral phases are charged per use. Checked arithmetic and
pre-allocation rejection remain. Work caps were not increased on the old path.
This new contract follows real complexity reduction and measured runtime.

For comparison, the old rejected Gaia pilot plan entailed about 4.01 billion
incident/output phase pairs, versus about 9.58 million pairs in the new worst-case
plan, despite finer internal angles and more wavelength nodes. Applying the
old pairwise operator at the newly accepted resolution would be more expensive
still. The new method does not hide that computation behind a lower counter.

## Fixed experiment and acceptance criteria

Source: original Galactic RING NSIDE128, seven 30 nm integrated photon bins,
360–570 nm, FlatPhotonDensity within each complete native bin. No catalogue
regeneration, subpixel flux, partial-bin spectrum or covariance is invented.
Observer: longitude −111.59989°, latitude 31.96403°, WGS84 ellipsoidal height 2097 m;
TT JD 2459679.9675055384, UT1 JD 2459679.966703672. View altitude 76.92569110795976°,
azimuth 271.8975544363482°. Atmospheric model column 2097–12097 m is independently
declared; numeric coincidence with ellipsoidal height asserts no geoid identity.
Pressure 779.5 hPa is a scenario, not measured exposure pressure. Tau550=.03,
Angstrom exponent 1.38, aerosol SSA 1, HG g=.65, no molecular absorption, black ground.

Current Siderust Bodhaine full-bin optics and historical-law/bin-center fixtures
are separate named experiments. Returned components are photons/(cm² ns sr).

The 1% independently validated synthetic continuum allowance is retained for
component-wise independent Gaia comparisons; it is not chosen to fit Gaia
residuals. Numerical acceptance separately requires diffuse angular/vertical
changes <=1%, kernel and HEALPix changes <=1e-4, wavelength changes <=1e-5,
order tail <=1e-6 times single+higher plus the existing absolute 1e-12 threshold
in the respective units. HEALPix needs two consecutive passing doublings.
The raw phase .001 ceiling is unchanged. Flux balance is an independent signed
diagnostic; it is not forced to zero. Successive-grid changes are not rigorous
continuum error or physical uncertainty bounds.

Accepted settings: boundary 128→256→512; diffuse 8→16 positive mu nodes and
32→64 azimuths; 4→8 subcells in the one homogeneous optical layer; max 32 orders;
wavelength 4→8 Gaussian nodes; kernel 128→256 mu intervals and 64→128 azimuths.
All criteria apply to separated first/higher contributions at each wavelength.

## Seven-band current-model results

Exact machine-readable values, per-wavelength convergence, provenance and
resource bounds: [current-model.jsonl](gaia-xp-factorized/current-model.jsonl).

| Band nm | TOA | Direct | Single | Higher | Scattered | Direct+scattered |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 360–390 | 0.00070866841832 | 0.00047875292598 | 0.00024976842258 | 0.00013736301521 | 0.00038713143778 | 0.00086588436377 |
| 390–420 | 0.0011481882899 | 0.00085792557413 | 0.00034440022677 | 0.00014014072612 | 0.00048454095289 | 0.001342466527 |
| 420–450 | 0.0015330519078 | 0.0012255264476 | 0.00036562194681 | 0.00011401920769 | 0.0004796411545 | 0.0017051676021 |
| 450–480 | 0.0020813172405 | 0.0017428399118 | 0.00040212894197 | 9.8778605597e-05 | 0.00050090754757 | 0.0022437474594 |
| 480–510 | 0.0021764040003 | 0.0018827000037 | 0.00036412970685 | 7.2305508108e-05 | 0.00043643521496 | 0.0023191352187 |
| 510–540 | 0.0022643794253 | 0.002004620425 | 0.0003444113798 | 5.6688486956e-05 | 0.00040109986676 | 0.0024057202917 |
| 540–570 | 0.0025144325099 | 0.0022632445165 | 0.000348413088 | 4.8565191062e-05 | 0.00039697827906 | 0.0026602227955 |
| **Sum** | 0.0124264417919 | 0.0104556098047 | 0.00241887371278 | 0.000667860740739 | 0.00308673445352 | 0.0135423442582 |

No component is missing or a substituted zero. Included orders range 10–18.

| Band | Maximum angular change | Maximum vertical change | Maximum discrete tail | Orders max | Max abs flux defect |
| --- | ---: | ---: | ---: | ---: | ---: |
| 360–390 | 1.9469e-09 | 3.3375e-08 | 1.4539e-11 | 18 | 2.2231e-05 |
| 390–420 | 1.1667e-09 | 2.8226e-08 | 1.5349e-11 | 16 | 2.8897e-05 |
| 420–450 | 1.4576e-09 | 1.9748e-08 | 1.6173e-11 | 14 | 2.917e-05 |
| 450–480 | 1.254e-09 | 1.5378e-08 | 1.4064e-11 | 12 | 2.3916e-05 |
| 480–510 | 1.2814e-09 | 1.0462e-08 | 1.4417e-11 | 11 | 1.7988e-05 |
| 510–540 | 2.0852e-09 | 7.7378e-09 | 1.3327e-11 | 10 | 1.3271e-05 |
| 540–570 | 2.5611e-09 | 6.2581e-09 | 1.3587e-11 | 10 | 9.8503e-06 |

Changes/tails above are spectral photon-radiance units; flux defect is relative.
Wavelength and two-axis kernel changes are retained separately in each record.
The largest signed-flux defect magnitude is 2.92e-5. Physical/model/source
uncertainty remains unavailable, not zero.

## Independent validation and physical decomposition

The independent generator lives in nsb-validation, not NSB runtime. Its
[eigenmode/Fourier reference](https://github.com/VPRamon/nsb-validation/blob/c21ce40/scripts/multiple-scattering/gaia_factorized_reference.py)
uses independently implemented HEALPix/GL-cell boundary moments, external
PythonicDISORT 1.8, and a separate layer-exact first-order integral. It consumes
only exported Siderust optics and explicit geometry, never NSB predicted light.
The scalar, black-ground, no-beam/no-emission/no-delta-scaling assumptions match.
The pinned dimensional-dispatch adapter fix changes no transfer equation.

NumPy 2.5.4 / SciPy 1.18.1 / Python 3.12.13, healpy 1.20.1 / Astropy 8.0.2; exact environment
and immutable source hashes are in the linked validation protocol. Raw warnings
and ill-conditioned synthetic conservative limits were retained. A 1e-10
absorption attempt failed independent stream refinement and was rejected as an
accuracy reference. The 1e-7 conservative-limit regularization is explicit;
1e-6→1e-7 changes the Gaia total by 2.45e-6 relative, much smaller than 1%.
This is controlled limiting evidence, not exact conservative eigensolver arithmetic.

The independent current-model 256-stream/512-boundary/8-wavelength total is
0.003086121304126204. NSB total differs by 0.0199%; every native-bin single
error is below 0.0670%, every higher-order error below 0.2454%, and all pass the
unchanged 1% component allowance. The full [comparison receipt](gaia-xp-factorized/matched-comparison.json)
fails closed on missing/rejected bands, mismatched source/mode or excess error.

Independent studies retained in [reference receipts](gaia-xp-factorized/independent-references.json):
128→256 streams changes total by 0.00534%; boundary 128→512 by 0.000539%;
wavelength 4→8 by 0.000148%; 16→32 Fourier modes is negligible at this view.
These are separate diagnostics, not a universal error proof.

The actual public Fourier solver also passes independent HG g=0.65 uniform,
absorbing, conservative-limit, two-layer and azimuthally varying synthetic cases.
Existing isotropic/Rayleigh/HG g=0.4 DISORT, second/third-order IVP, Fredholm,
first-order, thin/vacuum/absorption and long-tail regressions remain. Dense-versus-
Fourier sources agree to floating precision; known Legendre illumination has
the analytic HG/Rayleigh eigenvalues. Isolated-parent/pole/seam/horizon tests
preserve mass and mu moment; a layer-partition test uses identical optical cells.
No unsupported stronger-forward or arbitrary-table continuum accuracy is claimed.

Raw independent phase audit: the rejected old 8-mu/16-azimuth HG maximum is
1.0023848406843983. The new base 8/32 maximum is 1.000107475525399; the accepted
16/64 maximum is 1.000000008740596. View rows also pass unchanged .001.
[Exact audit](gaia-xp-factorized/phase-audit.json). No phase renormalization.

Separate totals (same original map):

| Experiment | Single | Higher | Scattered |
| --- | ---: | ---: | ---: |
| NSB current full bins | .002418873712783281 | .0006678607407389825 | .003086734453522263 |
| NSB current center optics | .002416540209088511 | .0006629454183695051 | .003079485627458016 |
| NSB historical-law center optics, geometric | .002598294175999691 | .0007318740997953487 | .003330168275795039 |
| DISORT historical centers, geometric | .002599060306155495 | .0007303971788670724 | .003329457485022567 |
| DISORT historical centers, apparent | .002599147087173253 | .00073045445746487 | .003329601544638123 |
| Frozen historical independent pilot | .002599075684947828 | .0007305265002674113 | .003329602185215240 |

The current full-bin versus current-center total changes by +0.2354%; current-
center versus historical-law center optics by −7.5276%. Geometric versus apparent
independent historical totals change by +0.004327%. At matched geometric historical
physics, finite-cell NSB versus independent continuum differs by +0.02135% total.
The NSB historical total is +0.01700% versus the frozen pilot; these differences
are separated, not falsely identified as equal physical models.

The independently regenerated apparent-place historical total is −0.00001924%
from the frozen pilot, well below its retained 128→256-stream .00601% change.
Its layer-exact first is also consistent with historical nsb2 single
.0025991471011773495. The older independent first used a finite GL-cell boundary
and first-order extraction, so subtracting that older first gives a slightly
different labelled higher-order contribution. That numerical-definition effect
is explicit; the historical 28.10% increment is never an input or fitted factor.

## Performance, measurement and reproduction

Linux x86_64 / Intel Core Ultra 9 185H / 30 GiB RAM / rustc 1.97.1, release,
locked dependencies, serial solver on a shared workstation. Compilation excluded.
Single observations are not controlled statistical speed guarantees.

The compact fixture has **three variable-width native bins**; original #269's
example evaluated only the first. Compare first-bin elapsed times: #269 18.79 s,
#270 streaming 18.69 s, projected .979 s. The three-bin streaming/projected commands
take 60.92/2.11 s. Direct matches exactly; projected first/higher differ by
−6.88e-6/−1.17e-5 relative against streaming at identical numerical controls,
inside the independently checked 1e-4 kernel target. Whole-process RSS is
8,132/3,824/12,772 KiB respectively (scopes differ as disclosed). Projection uses
more bounded workspace on this tiny case to reduce computational complexity.

The final full seven-bin current run takes 41.95 s (41.76 s user CPU);
the separately labelled center-optics runs take 31–33 s. Numerical heap instrumentation
measures 15,474,408 bytes after immutable source loading, with zero untracked
allocations; the final conservative six-plane workspace bound is 15,884,384 bytes, below
16 MiB. See [performance receipts](gaia-xp-factorized/performance.json). Source
JSON parsing dominates whole-process RSS and is explicitly outside solver
workspace. The optional glibc allocation tool is in nsb-validation; production
Rust remains entirely safe and has no dependency on the C profiler.

```sh
cargo run --release -p nsb --example starlight_multiple_scattering --locked -- \
  --projected --cells=4 --orders=32 --kernel-mu=128 --kernel-azimuth=64 \
  /path/to/starlight_360_570_candidate.json 2459679.9675055384 2459679.966703672
```

Append `--current-bin-centers` or `--pilot-bin-centers` only for the explicitly
labelled optical fixtures. The command verifies the original checksum/schema,
emits one component/convergence JSON record per bin, and fails if any criterion
fails. `--band=N` is an explicitly partial diagnostic, never a seven-bin result.
`--reference-input` consumes independently chosen wavelength-node lists on stdin
and exports only upstream optics and geometry for the reference generator.

Independent reproduction and fail-closed comparison are documented in
[the validation protocol](https://github.com/VPRamon/nsb-validation/blob/0801bce/scripts/multiple-scattering/GAIA-REFERENCE.md).
The optional full map is never downloaded or embedded by normal CI. All original
scientific assets and reserved observational holdouts remain untouched.

## Engineering, compatibility and remaining independent science

Public additions are one opt-in function and two numerical settings/result types;
there is no additional public map. Existing struct layouts and evaluator defaults
are unchanged. The API snapshot records additive changes. The old streamed path
remains available and resource-protected. Rust 1.89, full workspace/all-target/
all-feature tests, strict Clippy, Rustdoc, doctests, release, installed-wheel,
CLI, dependency/license/asset and archive checks are recorded on final PR head.
Local changed executable production coverage is **97.21% (940/967)** using the
existing tooling, above the requested 95%; no threshold or exclusion changed.
Full all-feature/all-target workspace tests pass (1,060 tests; 7 existing ignored),
as do 115 installed-wheel tests. Exact final-head Actions URLs are in the PR.

#187/#192 receive the completed numerical milestone and measured reusable
operator/source/phase reuse; general cross-query SiteWindowContext caching is
separate. No broad issue is closed automatically. #263 remains Outcome C /
admission Level 1: calibration, missing populations, variability, bright-star
completeness, unknown covariance/physical uncertainty, atmosphere/instrument
response and empirical admission remain independent. #265 publication blocker
and its negative registry-build guard remain unchanged.

This validates the specified scalar plane-parallel, black-ground Gaia numerical
experiment and the tested synthetic domain. It does not validate polarization,
spherical/refraction effects, arbitrary high-g/table accuracy, measured atmospheric
conditions, complete source populations, telescope-fiber radiance or empirical
sky-brightness improvement. Unknown uncertainties remain unknown.
