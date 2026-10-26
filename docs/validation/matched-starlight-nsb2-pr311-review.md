# PR #311 independent scientific review and remediation

Reviewed baseline: `817dd58d594738d2c2a2fe7e8a7149b36e52272e`, against base
`7d3e5dad3ed8bc4a2754353de03fdc6dddd645de` (`main`). All seven changed files,
resolved review discussions, CI, NSB map/evaluator/HEALPix interfaces, and nsb2
source paths were inspected. The pinned GerritRo checkout was cloned and checked
out at `bc9320db03fbff69997ce366c6e76a37339c5d00`; the initially installed
nsb2 Python files matched that checkout byte for byte.

## Findings and corrections

| Issue | Finding | Root-cause correction |
| --- | --- | --- |
| A, P1 | Confirmed: catalog integrated photon flux was relabeled radiance through `dimensionless_angles()`. The cone also differed from the NSB pixel. | Select catalog candidates about the NSB pixel center, retain exact Galactic RING pixel membership, integrate source photon flux and divide by the selected pixel's physical area. Diffuse rates retain their existing inverse solid angle. No angular equivalency is used. |
| B, P1 | Confirmed: an unexpected Level-2 exception became an error string in a successful output, with unconditional issue-closure claims. | Propagate failures; validate case completeness, finite results and metrics; serialize without NaN; write via a temporary sibling and atomic replacement. Level-1-only output has explicit skipped status, no closure claim and a separate default path. |
| C, P2 | Confirmed: the documented repository differed from the pinned implementation's declared Repository URL. Commands also left the shell in the nsb2 directory. | Verified and documented GerritRo/nsb2, its pinned commit, clean environments, NSB wheel build/install, transitive dependencies and data requirements. |
| D, P2 | Confirmed: `git_rev` only identified HEAD; dirty/staged changes and installed binary identity were absent. | Record runner checkout state and input digest, runner hash, explicit compiled-source build attestation, actual extension binary hash, dependency versions, NSB/NSB2 packaged assets and cached reference data. Generate official evidence from a committed implementation, then commit the JSON separately. |
| E, P2 | Confirmed: no CI invocation covered the benchmark and runtime-dependent tests could skip. | Add fast helper/artifact checks and a separate Python 3.12 job that builds NSB, installs pinned nsb2 and requires the actual paired operators and synthetic catalog tests. Missing dependencies cause failure in that job. No Gaia data downloads are needed. |

Changed files are the benchmark runner, its regression tests, CI workflow, the
benchmark methodology report, its numerical artifact, and this review report.
Production Rust physics, coefficients, calibration, defaults and public APIs are
unchanged.

## Scientific interpretation

The physical catalog observable is

`L_pixel = sum_i integral_300^650 F_lambda,i d_lambda / Omega_pixel`,

with membership determined before spectral reduction and
`Omega_pixel = 4*pi/(12*128^2) = 6.391586616190171e-5 sr`.
Catalog spectra after Pickles photometric normalization carry photon flux density
per wavelength; `SourceField.resolve_spectra` applies `Bandpass`, then
`ResolvedField.integrate` sums weights times spectra using nsb2's Simpson
wavelength integration. They have no collecting area or telescope throughput.
`HEALPixSource.from_photometric_map` already divides brightness weights by native
pixel solid angle, so its integrated output is radiance and must not be divided
again. nsb2's instrument projection is a later, separate operation with area and
acceptance weights, which this benchmark excludes.

Astropy's angle equivalency can insert/remove steradians numerically, but cannot
supply an observation aperture. For a nonempty field, the old omitted area factor
would differ from the correct pixel mean by `1/Omega = 15645.5675...`, even before
fixing the source selection. Its old `nside2resol` radius is sqrt(pixel area),
whose cone covers about pi times one pixel. A cone-area division would not repair
pixel-footprint matching.

The catalog rotation uses SOFA/ERFA, consistent with NSB/Siderust's convention.
Boundary rounding, especially at the frozen exact pole directions, can still
assign neighboring pixels; the recorded catalog pixel is authoritative for its
own mean, and exact NSB boundary-assignment equivalence is not claimed there.
The faint map remains an interpolation at the pointing rather than an integral
of the rotated NSB pixel. Every real-sky result therefore remains conditional.

The per-source median of finite Pickles alternatives matches nsb2's discrete
central instrument statistic before instrument weighting. It is a model
estimator, not an empirical radiance measurement or a confidence interval.
Any selected source with no available alternative fails the run; zero selected
sources in an available catalog is distinct from an unavailable population.
No common IDs establish XHIP/Gaia disjointness. Optional faint+bright excludes
XHIP; a population-complete bright+XHIP union is unsupported. The measured bright
file has 36,908,056 unique Gaia IDs and maximum G=14.999999. Assignment at exactly
G=15 and completeness of the aggregate faint asset cannot be independently audited.

Level-1 pairs NSB native-bin summation with nsb2 bandpass/Simpson integration of
the same imposed constant density. Each is checked against the analytic integral;
this is a genuine but restricted numerical operator comparison, not arbitrary
spectral equivalence. The fixture declares 12 NSB sources, one per pixel; nsb2
uses one sampled radiance element. The former inconsistent one-source declaration
was corrected. Mixed-unit Level-1 absolute errors are no longer averaged.

## Numerical evidence

The baseline suite passed all 26 tests. An independent baseline full run
reproduced the committed numerical aggregates:

| Metric | Baseline | Corrected default run |
| --- | ---: | ---: |
| Level-1 maximum relative difference | 1.927470528863119e-16 | 1.927470528863119e-16 |
| Level-1 imposed-spectrum NSB/nsb2 relative difference | 0 | 0 |
| Faint-only median relative difference | 0.8847676634377348 | approximately 0.884767663441 |
| Faint-only maximum relative difference | 0.9710947910998103 | approximately 0.971094791101 |

The corrected full default evidence contains 12 Level-1 cases (including four
paired-operator diagnostics) and 12 Level-2 cases (six directions, two conditional
configurations). Catalog XHIP selections are empty in these six pixels, so the
normalization fix cannot change their totals. Nonempty synthetic catalog tests
are the evidence for that correction. Removing the redundant apparent AltAz
round trip changes faint radiance by less than 1e-11. The optional full bright
BallTree was not run; its large population is audited from the cached file.
The JSON is the authoritative source for machine precision and dependency versions.

Documented population, spectral and geometric differences do not isolate causal
contributions to the large residuals. Neither empirical accuracy nor an NSB
radiometric calibration defect is demonstrated.

## Validation and reproducibility

Regression coverage includes analytic constant/linear spectra, HEALPix area,
NSB native-bin/partial-bin behavior, real paired NSB/nsb2 operators, known-flux
synthetic stars inside/outside a footprint, area scaling, empty fields, unavailable
templates, actual uniform `HEALPixSource` conversion, explicit wavelength coverage,
source-row accounting, failed/incomplete/nonfinite evidence, CLI exit status,
atomic output preservation, staged/unstaged/untracked Git states and dirty nsb2
pin policy. Optional bright-population assembly is checked to exclude XHIP.

The fast environment contains only NumPy and pytest: **40 passed, 7 skipped**;
runtime-dependent tests skip explicitly. Required integration mode: **47 passed,
0 skipped** in a clean Python 3.12 environment. A separate clean Python 3.12 environment installs pinned nsb2
from the documented VCS URL and the locally built NSB wheel; required integration
mode executes the operators without skips. A negative check with dependencies
absent confirms required mode fails rather than skipping. Related observational
statistics and product-documentation tests passed: **18 tests and 5 subtests**.
The explicit `--skip-level2 --offline-audit` command also completed successfully.
The modified workflow is checked with actionlint 1.7.7.

Full benchmark data are accessible in the Astropy cache: faint Gaia map
301,990,016 bytes, bright Gaia catalog 2,362,115,904 bytes, SVO Gaia and Johnson
passbands and CALSPEC Vega. Their bytes, bundled Pickles/XHIP data and NSB runtime
assets are fingerprinted in the artifact. Reproduction commands and the clean
implementation/evidence two-commit workflow are in
[the methodology report](matched-starlight-nsb2-v1.md#8-reproduction-and-provenance).

The old SHA `0a7a0c029a2748e73b8355f3a77297cc22fadebd` is an ancestor of the
reviewed head and predates substantial benchmark changes. The exact historical
working-tree contents and binary/environment cannot be recovered because they
were never recorded. Reproducing its numbers does not establish that missing
historical provenance. The new evidence records its actual generating inputs.

## Merge recommendation

**READY FOR MERGE** on the corrected restricted Outcome B contract, subject to
normal PR CI. The five confirmed defects are addressed; no population-equivalent
real-sky or empirical validation claim is made. Remaining scientific follow-ups
are source-ID-certified populations, a truly common diffuse angular operator,
external XP 360–570 nm asset availability and separately validated atmospheric
transport. They are explicit scope limits rather than fitted corrections.
