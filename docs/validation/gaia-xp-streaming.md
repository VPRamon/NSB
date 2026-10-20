# Gaia XP boundary streaming: delivered change and remaining gates

This is **partial implementation evidence**, not completion of the atmospheric
transport roadmap. The full seven-band multiple-scattering experiment remains
uncomputed and independently unvalidated. No empirical admission changes.

Initial main: `af41a8613c22b5183609086402af7e3b13b44484`. GitHub reported
PR #269 merged at 2026-10-20 08:35:48 UTC before the new branch was created.
Branch: `feat/gaia-xp-multiple-scattering-reference`, based directly on that main.
The original dirty checkout was preserved separately. Final revision, PR and
hosted-check links are recorded in the PR, avoiding self-referential hashes.

## Architecture and scientific audit

The authoritative implementation is `transport/multiple.rs`: scalar,
plane-parallel, finite-height-cell successive physical scattering orders,
full-sphere diffuse ordinates, black boundaries. Its Siderust optical column,
Optica phase laws, direct illumination, bidirectional formal sweeps, component
separation, raw phase audit and order-tail criterion are retained. Only external
boundary traversal and source storage change. No second solver, map API, optics
model, crate, dependency, public API or evaluator-default change is introduced.

`StarlightSky` already supplies the checked Galactic/local basis through
Siderust's canonical fixed-frame transform and explicit TT/UT1. Its immutable
map supplies native RING parent integrals and original checksum/provenance.
Refined pixel centers retain parent density `I_bin / bin_width` and exact
solid-angle weights; refinement integrates transport kernels, not invented
subpixel photometry. The original center horizon quadrature is retained and
requires independent convergence. WGS84 ellipsoidal observer height and the
independently declared atmospheric model height are not silently identified.

The old boundary vector reserved full-sphere capacity even though it stored
only visible cells. At NSIDE256 its capacity alone was 37,748,736 bytes
(786,432 × 48), larger than 16 MiB. The replacement visits increasing RING
indices with one transient direction, projects attenuated light directly into
the existing diffuse/view source fields, and accumulates direct photon flux
in the same traversal. The redundant retained first-source copy is removed.
No inter-evaluation or inter-wavelength caches are added. Each of the four
refinement solves streams the geometry separately, and preflight now charges
all four passes. Phase kernels remain reused over vertical cells within a
sample; wavelength-dependent optics/phases are recomputed.

Memory now scales as `O(D² + K D)` rather than `O(B + D² + K D)` for boundary
cells B, diffuse directions D and vertical subcells K. CPU remains
`O(B K D + N K D²)` for N orders. Checked requested-buffer accounting includes
phase matrices, five conservative field planes, direction/cell records,
view/factor buffers and concurrent wavelength/cosine quadrature. Immutable
source assets, result metadata, allocator bookkeeping and thread stack are
outside this numerical workspace estimate. Both CPU caps and the 16 MiB
workspace ceiling remain enforced; no unbounded allocation is admitted.

## Exact reference equivalence and measured performance

Linux x86_64, Intel Core Ultra 9 185H, 30 GiB RAM, shared workstation;
`rustc 1.97.1 (8bab26f4f 2026-07-14)`, Cargo release, locked dependencies.
Single process, existing solver is serial. `/usr/bin/time -v` measured execution
of already-built binaries; compilation excluded. Baseline was built directly
from the exact main SHA above in a detached checkout. These are one observation
per revision, not statistically controlled speed estimates.

Same existing synthetic fixture, complete 360–390 nm bin, Rayleigh depth .08,
absorption depth .08, mu_view=.8, azimuth=23°, base mu=4, azimuth=4,
cells=2, orders≤8, boundary NSIDE8→16→32→64→128, wavelengths 2→4:

| Measurement | Main #269 | Streaming |
| --- | ---: | ---: |
| Wall seconds | 20.62 | 18.55 |
| Peak whole-process RSS KiB | 8,184 | 3,764 |
| Conservative full-bin work units | 1,610,948,224 | 1,686,371,968 |
| Dominant requested workspace at NSIDE128 | 9,719,808 bytes (old estimate) | 292,288 bytes (expanded estimate) |
| Accepted boundary NSIDE | 128 | 128 |
| Accepted diffuse directions | 128 | 128 |
| Accepted vertical cells | 4 | 4 |
| Included physical scattering orders | 7 | 7 |

RSS includes executable/library/input/runtime pages and is not the numerical
workspace allocation. Unused reserved pages need not contribute to RSS. The
new work estimate is larger because it honestly charges repeated geometry;
streaming alone does not improve asymptotic CPU complexity.

All five integrated radiances are bit-identical in this comparison:

| Component, photons/(cm² ns sr) | Both revisions |
| --- | ---: |
| Direct | 0.002456192259233946 |
| Single | 0.00010635495654512698 |
| Higher | 0.000012529779958483553 |
| Total scattered | 0.00011888473650361054 |
| Direct plus scattered | 0.0025750769957375566 |

Flux balance changes from −0.0001266470069952773 to
−0.00012664700699579497 (5.18e-16 absolute), owing to direct-absorption
summation grouping. The regression permits 1e-13 absolute, far below the
measured midpoint-cell closure defect. Angular/vertical diagnostics and tail
are unchanged: 2.380161430844262e-9, 8.278106537778231e-9 and
9.827531048214635e-12 spectral photon-radiance units, respectively.
These grid differences are diagnostics, not continuum error bounds.

Compact tests compare every streamed direction and weight exactly with the
materialized #269 generator for uniform and isolated-parent skies. Source,
first/higher radiance, order count, discrete contraction/tail and photon flux
agree exactly against the stored traversal at all four resolution combinations.
The pre-existing independent DISORT, E1/Fredholm, order-specific IVP and
wavelength-integrated fixtures remain applicable to their documented domains.
Parent conservation through native NSIDE8, poles/seams, hemisphere measure,
vertical/angular/order/wavelength refinements and strict rejection tests remain.
A full NSIDE512 stream is tested without retaining its 1,572,864 visible cells.

## HG g=0.65: evidence and supported-domain limit

At the finest supported **base** diffuse grid (8 positive mu nodes, 16 azimuths),
raw HG g=.65 row sums range from 1.0000783654168557 to
1.0023848406843998. The maximum exceeds the unchanged 1.001 ceiling. This
is a diffuse-phase quadrature failure; resolving the HEALPix boundary does not
repair it. Tests preserve the rejection without renormalizing the kernel.
The four-grid API must pass the base phase audit before returning any result.

An additional independent absorbing synthetic reference uses the pinned
`nsb-validation` eigenmode/Fourier DISORT generator with g=.65. A single finer
internal diagnostic can be compared within the existing 1% synthetic continuum
allowance. This diagnostic does not establish an accepted public four-grid
plan, conservative-atmosphere convergence, an anisotropic Gaia solution, or
observational accuracy. Inputs/stream refinement are retained in
`crates/nsb/tests/data/multiple-scattering/hg65-v1.json`.

At 256 DISORT streams, first/higher are 0.002060142368318644 /
0.00014797698961003733. The independent 128→256 higher-order change is
1.685e-6 relative. The NSB single-grid diagnostic uses 8 mu × 32 azimuths
per hemisphere, 16 cells and ≤24 orders (211,353,600 work units, <16 MiB),
and returns first/higher 0.0020600174581757295 / 0.00014800469904260885:
−0.00606% / +0.01873% relative. Eleven physical orders are included, with
discrete omitted-order tail 1.0278733673396398e-11. The public API still rejects
this unsupported four-grid configuration; this is deliberately tested.

Reproduce the independent fixture from validation revision
`08b50e604589702405267f6b1ebae521dfaa1b24` with Python 3.12.13,
NumPy 2.5.4, SciPy 1.18.1 and PythonicDISORT 1.8, `OPENBLAS_NUM_THREADS=1`.
Import `scripts/multiple-scattering/disort_reference.py` using `runpy.run_path`,
then call its `evaluate([[0., .12, .7, .02]], .65, streams)` for streams
32,64,128,256. The fixture pins the generator SHA-256, parameters and packages.
The first-order reference is a separate layer-exact 256×512 integral;
stationary transfer is external DISORT without NSB midpoint cells.

## Frozen seven-band attempt and exact remaining blockers

Original 83,561,301-byte source checksum was reverified, without modification:
`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
The existing loader validates schema, Galactic RING grid, original seven bins,
TOA origin and provenance. No map bytes are added to normal CI.

```sh
cargo run --release -p nsb --example starlight_multiple_scattering --locked -- \
  /path/to/starlight_360_570_candidate.json \
  2459679.9675055384 2459679.966703672
```

The example now evaluates every native bin, emits JSON component/convergence
records on success or explicit rejection records with absent radiances, and
exits unsuccessfully if any bin fails. Its declared site/atmosphere remain the
existing reproduction scenario: −111.59989°/+31.96403°, ellipsoidal height 2097 m,
separately declared model column 2097–12097 m, pressure 779.5 hPa scenario,
Siderust Bodhaine Rayleigh, tau550=.03, exponent1.38, SSA1, HGg=.65,
no molecular absorption, black ground, original geometric viewing direction.
Pressure and datum coincidence are assumptions, not exposure measurements.
The example is a diagnostic reproduction, not a full mission-compliant accepted
Gaia command or independently validated physical profile.

| Native bin nm | Direct | Single | Higher | Acceptance |
| --- | --- | --- | --- | --- |
| 360–390 | unavailable | unavailable | unavailable | CPU preflight rejected |
| 390–420 | unavailable | unavailable | unavailable | CPU preflight rejected |
| 420–450 | unavailable | unavailable | unavailable | CPU preflight rejected |
| 450–480 | unavailable | unavailable | unavailable | CPU preflight rejected |
| 480–510 | unavailable | unavailable | unavailable | CPU preflight rejected |
| 510–540 | unavailable | unavailable | unavailable | CPU preflight rejected |
| 540–570 | unavailable | unavailable | unavailable | CPU preflight rejected |

The original memory rejection is removed. This configured plan now requires
**25,699,507,840 work units per native bin**, **6.425 times** the four-billion cap,
before any transfer or phase solve. Even the smallest allowed 2-mu/4-azimuth,
2-cell/2-order plan with NSIDE128→256→512 and six wavelength evaluations has
more than 13.77 billion boundary work units; changing just settings cannot
complete this product under the current resource contract. The whole-process
RSS during source loading is not an accepted solver workspace measurement.
No seven-band radiance sum, accepted Gaia convergence, Gaia tail/flux closure,
Gaia wall time or historical multiple-scattering residual can be reported.

Required further implementation is a validated reduction in boundary-projection
complexity and a memory/work-bounded diffuse operator resolving HG .65, including
its conservative-column order criterion. Neither a larger resource allowance
nor silently relaxed phase/convergence tolerance was substituted. Full current
Gaia DISORT comparisons, historical-law/bin-center replay and separate optical,
spectral, astrometric and residual numerical effects still require actual
accepted multiple-scattering calculations.

## Historical provenance and scope reconciliation

The [merged first-order report](starlight-transport-267.md#frozen-pilot-boundary-and-reproducibility)
recovers the pinned original validation protocol, exposure midpoint, epochs,
historical Rayleigh law, source/solver identities and frame differences.
Historical first/total references are 0.0025990756849478282 /
0.0033296021852152395; higher is 0.0007305265002674113. They were never inputs
to this implementation. Inherited first-order current-full-bin/current-center/
historical-center results are 0.00241960698610553 / 0.00241726783764973 /
0.00259905749633912. They are not newly measured multiple-scattering results.
Full-bin first order differs from current centers by +.0967683%; current versus
historical optical law differs by −6.99445% at matched centers. Prior all-pixel
geometric/apparent residuals are .0847/17.8393/21.5749 arcsec (min/median/max).
These inherited diagnostics cannot quantify the missing multiple-scattering
model decomposition. No compatibility tolerance is chosen to fit a residual.

#187 already has merged direct (#193), single (#266), spectral Starlight (#268)
and restricted synthetic multiple-scattering (#269) evidence. Full Gaia and
independent matched pilot acceptance remain unfinished; keep it open.
#192 has conservative boundary sampling, source/phase reuse over vertical
cells, this bounded-memory change, equivalence and a small benchmark. General
kernel/cache/LUT and compatible repeated-query acceleration are not delivered;
keep its unmet requirements. #263 remains Outcome C / Level1; Gaia systematics,
missing populations, bright-star completeness, uncertainty propagation,
instrument response and empirical admission are separate unresolved work.

#259/#260 are audited against existing generic v2 schema/configured variable
edges and the single version-dispatched `StarlightMap` Rust/Python API from
#257/#268. This PR does not duplicate those capabilities or establish all of
those issues' historical acceptance checks; no closure is justified here.
#265 remains the temporary Git-pin/registry-publication blocker. No release
requirement or publication safeguard is altered.

Unknown physical/observational uncertainties remain unavailable. Scalar,
plane-parallel, black-ground, unpolarized and finite-cell approximations remain
explicit. No source selection, calibration, defaults or reserved holdouts change.

## Engineering and remaining completion gates

Changed executable production-line coverage measured with the existing LLVM
and diff-gate tooling is **70/70 = 100%**, above the requested 95%, with no
exclusion/threshold changes. Collection used the focused multiple-scattering
suite; full hosted workspace coverage is separately checked. Reproduce:

```sh
cargo +nightly-2026-09-02 llvm-cov -p nsb --lib --locked --no-report -- transport::multiple
cargo +nightly-2026-09-02 llvm-cov report --lcov --output-path coverage.lcov
scripts/coverage-gate.sh diff --lcov coverage.lcov --base origin/main
```

The PR records final-head local test commands and hosted checks. No claim is based on an older green SHA. Installed-wheel tests exercise
the unchanged Python boundary; Rust 1.89, CLI, public API and archive guards are
checked separately. New production logic remains private and fallible.

| Mission gate | Status |
| --- | --- |
| A: accepted full seven-band Gaia computation | Not met: CPU guard, then diffuse phase resolution |
| B: five independent Gaia convergence dimensions | Not met; synthetic evidence is not Gaia evidence |
| C: matched full-map independent and historical comparison | Not met |
| D: practical full Gaia bounded CPU/memory | Memory allocation fixed; full CPU viability not met |
| E: truthful provenance, uncertainty, maturity/defaults | Preserved |
| F: engineering | Exact final measurements and CI recorded in PR |

**SCALABILITY IMPLEMENTED — SCIENTIFIC VALIDATION INCOMPLETE**
