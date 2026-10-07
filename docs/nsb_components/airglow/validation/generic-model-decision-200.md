# Generic optical Airglow admission decision (#200)

Status: Phase 1 completed; **no replacement admitted; implementation stopped**.
Reviewed: 2026-09-29.
Repository audited: `d4e2672303553cf35056eb2407e4b0456ca0e3c0`.
Authority: [issue #200](https://github.com/VPRamon/NSB/issues/200), read in full,
and the accompanying implementation brief. The issue had no comments at review.

## Decision

The sources reviewed below do not establish a ready, reproducibly generatable,
geographically portable **complete 300–650 nm nighttime Airglow prediction**
with independent multi-site validation and quantified residual variability.
No generic model, runtime dataset, numerical uncertainty, or site calibration
is selected. This is an admission decision about the evidence examined, not a
claim that such a model is physically impossible or that the literature search
proves none exists.

The strongest research inputs are optical WINDII and OSIRIS observations,
with component-specific chemistry as a possible route to a future synthesis.
Neither a line-only product nor an infrared-derived surrogate can presently
replace the integrated optical component without hiding missing emission.
PALACE is substantially more complete spectrally, but adopting its empirical
climatologies unchanged would repeat the site-dependence problem.

The implementation brief explicitly requires stopping before inventing a model
when Phase 1 cannot justify one. Accordingly, Phases 2–6 have **not** been
implemented. Existing runtime code and assets remain unchanged, including
`ParanalNollSkyCalcFors1` and the automatic Paranal fallback. Their continued
presence is unresolved #200 debt, **not** an accepted legacy architecture or
permission to release that architecture as the solution. No old numerical
equivalence requirement is introduced.

## Scientific-data rule

NSB core contains observatory-independent scientific models and reproducibly
generated datasets. Observatory-specific empirical refinements are external,
explicit, provenance-bearing calibration products. This is the target admission
rule; the current Airglow asset does not satisfy it.

Data provenance may include particular observatories without making a model
site-specific. What matters is whether the runtime scientific product depends
on site identity for its meaning. Changing coordinates, pressure, or geometry
does not establish the portability of fitted emission coefficients.

## Candidate assessment

“Not established” below is an unresolved admission field, not a zero error or
an assertion that the source has no documentation. No raw observation files
were downloaded, reduced, or checksum-pinned in this survey. No new dataset is
redistributed. Article licensing is not assumed to license underlying data.
The referenced versions are those assessed, not assertions of latest releases.

### Spectral, geographic, altitude, and time coverage

| Candidate | Observable and wavelength/species | Geography and altitude interpretation | Time and solar-cycle coverage |
| --- | --- | --- | --- |
| Existing Noll/SkyCalc continuum | Relative continuum plus empirical amplitude; bundled grid 300–2500 nm, runtime integral 300–650 nm | Paranal-derived; one assumed 90 km shell | Six double-month seasons × three night thirds plus aggregate bins; fitted linear F10.7 response; exact imported release unknown |
| PALACE v1.0 | 300–2500 nm; nine species, lines and three continua | Empirical Paranal climatology; species-dependent emission | X-shooter October 2009–September 2019 plus UVES; seasonal/local-time, solar-response and residual-variability classes |
| WINDII archive and 553.1 nm continuum study | Optical-line VER, including 557.7/630.0 nm; narrow 553.1 nm continuum channel | Broad satellite coverage; altitude-resolved limb inversion, not ground radiance | Mission 1991–2003; product-specific night sampling must be audited. The continuum study uses 18 nights in November 1992–January 1993, 42°S–42°N, 20–04 local solar time; that subset does not cover a solar cycle |
| Odin/OSIRIS optical limb spectra | Spectrograph 280–810 nm; green line, Na, O2 and visible continuum evidence | Limb spectra and retrieved profiles; selected low-latitude 75–105 km data in the FeO study | Mission observations and study subsets differ; exact admissible night/season/solar sampling not established here; Sun-synchronous sampling cannot be treated as uniform local-time coverage |
| TIMED/SABER v2.0 | IR OH 1.6/2.0 µm, O2 1.27 µm, atmospheric-state retrievals | Limb profiles; latitude sampling alternates roughly 53° in one hemisphere to 83° in the other | Observations began 2002; multi-year/solar-cycle coverage available, with yaw/local-time sampling coupling |
| GOMOS night-emission retrieval | Published O2 762 nm and OH 930 nm limb emission retrievals | Satellite sampling; tangent radiance requires inversion/geometry treatment | Envisat-era decade; illumination and local-time selection affect coverage; no admitted optical solar-response model |
| WACCM6 with modified metal chemistry / Noll et al. 2024 | Simulated component emission and observed 300–1800 nm pseudo-continuum | Global chemistry capability; published spectral/variability comparison centred on Paranal | Observations 2009–2019; dedicated simulations 2003–2014; no validated portable optical predictor grid established here |
| NCAR GLOW, documentation v0.981 | Thermospheric excitation/ionisation, densities and emission rates | Physical atmosphere/electron-transport calculation with altitude grids | Model inputs determine date, atmosphere and activity; not itself a multi-year measured optical continuum dataset |
| NRLMSIS 2.0 plus green-line chemistry | Neutral densities/temperature converted by additional reaction kinetics to 557.7 nm VER | Geographically parameterized atmosphere; published Tory/Siberia comparison | The assessed regional study uses 2017–2021; not a full solar-cycle, multi-site absolute optical validation |
| ICON/MIGHTI L2 | Oxygen green/red relative VER ancillary to winds | Limb-derived profiles at mission-accessible latitudes; relative brightness is not an absolute radiometric standard | Full-mission files available; a per-version night/solar sampling inventory is not established here |

### Calibration, uncertainty, access, rights, and disposition

**Existing Noll/SkyCalc asset — reject as generic baseline.** The manifest
already records unknown historical release, unrecorded upstream licence, and
unreproducible fabrication. Current checksums prove byte integrity only. Its
empirical uncertainties are not cross-site prediction errors. The local
`nsb-data-tools` copy pipeline does not reconstruct observations or fits.
[Noll et al. 2012](https://doi.org/10.1051/0004-6361/201219040) supplies the
scientific lineage; the detailed implementation audit below uses repository
evidence, not an asserted reconstruction of that paper's data.

**PALACE — reject unchanged core use; retain as potential source/calibration
research.** Flux calibration uses response curves and standard stars; agreement
with source-site spectra and residual classes is valuable but does not establish
multi-site portability. Code/data access is versioned through
[Zenodo 14064022](https://doi.org/10.5281/zenodo.14064022). The authors specify
CC BY 4.0 for data and GPLv3 for code; local offline evaluation is possible.
No files/checksums or NSB generator were admitted. Paranal calibration would
still require residuals against the eventual generic model and independent
held-out observations, rather than copying these coefficients into a model
variant. [Model paper and availability statement](https://gmd.copernicus.org/articles/18/4353/2025/).

**WINDII — shortlist for optical components; defer runtime admission.** CSA
provides HDF data and supporting documentation under Open Government Licence –
Canada. Its archive describes instrument validation; numerical absolute-error
budgets, retrieval revisions, flags, and independent optical radiance residuals
must be extracted for the selected files. A deterministic offline climatology
is feasible in principle, but no reduction or fit was performed here.
[CSA archive](https://data.asc-csa.gc.ca/en/dataset/4501efc8-9fe0-4a52-9a60-f2657a969c0b),
[filter/product description](https://donnees-data.asc-csa.gc.ca/users/OpenData_DonneesOuvertes/pub/WINDII-data-archive/Supporting-Documents/WINDII_WholeDataSet_Description_EN.pdf).
The 553.1 nm study removes extraterrestrial backgrounds and assumes NO+O
chemistry; a narrow continuum measurement does not determine the full optical
spectrum. Its limited temporal sample cannot supply an annual/solar-cycle law.
[von Savigny et al. 1999](https://doi.org/10.1007/s00585-999-1439-9).
Do not substitute the published WINDII **dayglow** F10.7/SZA model for nightglow.
[Shepherd et al. 2021](https://doi.org/10.1029/2020JA028715).

**OSIRIS optical — shortlist; defer runtime admission.** The FeO study uses
calibrated spectra, subtraction of identifiable bands, and limb inversion;
the 480–530 nm order-sorter region is omitted. Noise and chemical attribution
are not a validated broadband prediction uncertainty. No all-season, all-night
optical product was pinned here. [Evans et al. 2010](https://doi.org/10.1029/2010GL045310).
The provider documents Level 1 services and Level 2/3 downloads; free research
access/registration alone is insufficient evidence for redistribution of a
new optical product. Exact Level 1 revision, file list, licence and calibration
history remain admission tasks. Offline use requires a staged snapshot and
new reproducible reduction. [Provider access](https://research-groups.usask.ca/osiris/data-products.php).
Recent OSIRIS/MATS comparisons concern the oxygen atmospheric band, not
independent validation of a complete 300–650 nm prediction.
[Joint observations](https://doi.org/10.5194/amt-18-4453-2025).

**SABER — reject as direct optical baseline; retain as atmospheric/component
research input.** Infrared radiometry and retrieved chemistry are not measured
optical continuum. Retrieval uncertainties and high IR signal-to-noise cannot
be transferred to optical errors. Cross-band chemistry would need its own
validation. Versioned files can be staged for offline use; selected-file
checksums and redistribution terms are not established in this review.
[Official data access/version information](https://saber.gats-inc.com/data.php),
[instrument algorithms](https://spdf.gsfc.nasa.gov/pub/data/timed/saber/documentation/Level2B.pdf),
[v2.0 study and sampling](https://doi.org/10.1002/2017JA023966).

**GOMOS — reject assessed retrieval as optical baseline.** The published
calibrated limb retrieval has error estimates and screening, but the selected
emissions lie outside 300–650 nm. Its stated future intercomparison is not a
completed independent optical validation. Exact input processing version,
redistribution permission and reproducible NSB subset are unestablished.
Offline retrieval would require an ESA Level 1 snapshot and implementation of
the published reduction; instrument spectral coverage alone is insufficient.
[Bellisario et al. 2014](https://doi.org/10.1175/JTECH-D-13-00135.1),
[ESA collection catalogue](https://catalog.maap.eo.esa.int/doc/stac.html).

**WACCM6/continuum study — defer as research, reject ready baseline.**
The modified chemistry supports distinct visible and IR emitters; the visible
simulation underpredicts total emission and needs additional mechanisms.
Absolute optical closure and independent multi-site residuals therefore remain
unestablished. The study links ESO raw data and
[Zenodo 8335836](https://doi.org/10.5281/zenodo.8335836); the dedicated simulation
results are stored at Leeds. Pinning the modified chemistry, forcing, full
configuration and output is necessary for a deterministic offline product.
The paper is CC BY 4.0; the exact simulation/code/input redistribution contract
has not been verified. No numerical uncertainty is adopted.
[Noll et al. 2024](https://doi.org/10.5194/acp-24-1143-2024).

**GLOW — reject as complete optical baseline; possible thermospheric component
research.** Source and example drivers are downloadable, permitting offline
calculations with pinned inputs. Its thermospheric scope does not by itself
close mesospheric Na/O2/continuum emission or supply NSB residual uncertainty.
No relevant multi-site broadband validation was established. The upstream
licence grants research/academic/nonprofit use with restrictions; do not infer
a permissive licence from wrappers. Commit, external atmosphere inputs,
redistribution of derived assets and uncertainty would require separate review.
[NCAR documentation](https://github.com/NCAR/GLOW/blob/master/Glow.txt),
[upstream licence](https://github.com/NCAR/GLOW/blob/master/Glowlicense.txt).

**NRLMSIS + kinetics — defer as green-line research.** NRLMSIS is a neutral
atmosphere model, not a calibrated spectral emission product.
[Official model description and source access](https://ccmc.gsfc.nasa.gov/models/NRLMSIS~2.0/).
The regional study converts FPI arbitrary intensity units using SABER-derived
green-line estimates. Agreement after that conversion is not independent
absolute calibration of the same estimates. Reported regional comparisons
cannot establish full-band, multi-site residuals. Code/reaction version,
forcing files, reproduction config, licences and propagated kinetic/model
uncertainty remain unpinned; an offline calculation is technically possible.
[Tory study](https://doi.org/10.3390/app13085157).

**ICON/MIGHTI — defer for layer/relative-variability research; reject as an
unqualified absolute baseline.** Official ancillary fields are explicitly
named *relative* VER, with statistical errors and quality flags. Wind
validation must not be relabelled absolute-radiance validation. Downloadable
versioned files support offline analysis; exact selected release, absolute
radiometric transfer, redistribution contract, and propagated errors remain
unestablished. [Mission data](https://icon.ssl.berkeley.edu/Data),
[L2 red-line field definitions](https://spase-metadata.org/NASA/NumericalData/ICON/MIGHTI/L2/Vector/Red/PT30S).

### Why a combination is not yet an adopted model

A possible synthesis would need optical-line amplitudes, separate continuum
bases, and altitude/temporal response per component. Splicing available mean
spectra and global line data does not establish shared absolute calibration,
sampling corrections, covariance, spectral completeness, or portability.
In particular, a narrow 553.1 nm continuum anchor cannot set the missing blue
continuum, and neither a 557.7 nm measurement nor IR OH fixes the red-line
thermospheric contribution. These are inferences from the mismatched
observables above; they are the reasons admission remains deferred.

No supported geographic, temporal, F10.7, zenith or wavelength domain is
declared for a replacement. The requested 300–650 nm band is a requirement,
not an achieved applicability domain. No universal shell, common scalar law,
solar averaging convention, uncertainty percentage, or geomagnetic correction
is selected.

## Audit of the existing asset and full evaluation path

The checked asset SHA-256 is
`d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f`.
Its manifest schema is `skycalc-airglow-continuum-v1`. The manifest honestly
records unknown source release/licence and historical import. Do not reinterpret
its checksum or its local copying config as fabrication provenance.

| Quantity | Current meaning/classification | Required disposition on admission |
| --- | --- | --- |
| 46-point continuum shape, normalized at 0.543 µm | Paranal-derived empirical spectrum; fixed shape is a convenience assumption | Replace; retain source evidence only if a justified research/calibration use exists |
| Absolute normalization / `global_scale = 79.829` | Empirical intensity normalization; parsed as a scale, then interpreted in SkyCalc photon radiance units | Replace, not a universal physical constant |
| Wavelength uncertainty `drflux` | Empirical shape scatter; zero at the normalization anchor is not perfect absolute knowledge | Replace with component covariance/error evidence |
| Mean and sigma `4 × 7` matrices | Joint Paranal night/season coefficients, including aggregate row/column | Replace; these are not independent universally factorable seasonal/night laws |
| Six double-month bins | Model-specific empirical convention | Remove from generic domain semantics |
| Three astronomical-night thirds | Model-specific empirical convention, derived using generic solar events | Remove bins; retain reusable astronomical time/event infrastructure |
| F10.7 intercept `0.2068` and slope `0.006139` | Paranal-fitted response, shared across continuum wavelengths | Replace; retain independent F10.7 resolver/store/provenance |
| 90 km height | Effective-height assumption in both asset and default geometry config | Remove as universal source property; select component heights from evidence |
| Van Rhijn geometry / vertical integration | Generic spherical emitting-volume approximations under their explicit hypotheses | Retain capability and numerical convergence tests; geometry validity does not establish source validity |
| Noll effective extinction | Published radiative-transfer fits, not measured continuum coefficients, but dependent on source-layer/atmosphere assumptions | Reassess under #187; do not carry coefficients into all components by default |
| Site-profile/user scale | Operational multiplicative assumption without residual evidence | Do not promote to calibration; replace only with a scientifically defined correction if supported |
| µm→nm and radiance unit conversion | Generic dimensional convention | Retain and test independently of old data |
| Linear spectral interpolation and endpoint clamping | Numerical choices for this table | Reassess representation/domain; clamping is not scientific extrapolation permission |
| B=445 nm, V=551 nm and S10 conversion | Repository monochromatic diagnostic convention, not Johnson-band integration | Retain clearly labelled proxy where relevant; not validation of Airglow physics |
| Legacy selector, automatic fallback, SkyCalc parser | Artifacts of the old runtime architecture | Remove after replacement admission; never retain a supported reference/compatibility branch |

The present call chain is:

1. `build/types.rs` requires the asset/schema; build validation checks SHA-256
   and produces bundled metadata. `components/airglow/calibration.rs` embeds
   bytes, validates fixed tables and spectra, and constructs `AirglowContinuum`.
   Despite the filename, this is model data, not #200's external calibration.
2. `evaluator/types.rs` defaults to automatic selection;
   `components/airglow/selection.rs` resolves automatic and explicit requests
   to the same Paranal variant. `evaluator/core.rs` eagerly loads the continuum
   into an `Arc` and later resolves atmosphere, F10.7 and site-profile scale.
3. `model.rs` initially derives geometry from the asset height; evaluator
   configuration then supplies its selected geometry. Its default separately
   encodes 90 km in `geometry/van_rhijn.rs`. Both dependencies need review.
4. `temporal.rs` computes local-solar month and astronomical-night thirds.
   Unbounded night selects the aggregate night bin. Generic solar-event calls
   are reusable; the table indexing is not a generic climatology.
5. `continuum.rs` validates inputs, then returns the current
   `OutsideAstronomicalNight` physical-zero outcome outside its night gate.
   This is an existing modelling convention, not evidence that atmospheric
   emission physically ceases at twilight. #151 must distinguish unsupported
   source domain from genuine physical zero in the replacement.
6. In-domain emission multiplies shape by normalization, linear F10.7,
   joint night/season coefficient, geometry and operational scale. Spectral
   Noll propagation precedes 300–650 nm integration and B/V samples. The
   integrated-only threshold path uses the same scientific ingredients with
   prepared night intervals; it must migrate too, not only point queries.
7. Uncertainty combines temporal sigma/mean with integrated absolute spectral
   sigma in quadrature. Integrating sigma is not a measured wavelength
   covariance model; neither component supplies cross-site residual uncertainty.
   `evaluator/metadata.rs` propagates selection, source and maturity; CLI and
   Python surfaces and their tests expose the old identity.

The Noll stage uses `X=(1−0.972 sin² z)^−1/2`,
`f_R=1.669 log10(X)−0.146`, `f_M=1.732 log10(X)−0.318`, and
`exp(−X(f_R τ_R+f_M τ_M))`. Negative factors near zenith represent light
scattered into the beam, not plain direct attenuation. Local-pressure handling
avoids double altitude reduction, which is independently reusable dimensional
physics. The existing implementation documents a fit mainly through 60° and
extrapolates beyond it; molecular absorption is absent. Retaining numerical
formula tests does not establish portability across source layers. Airglow
must remain outside the celestial `TopOfAtmosphere -> DirectTransmission`
route. [Current transport contract](../../../specifications/atmospheric-transport.md).

## Repository occurrence audit and migration ownership

[The occurrence inventory](generic-model-occurrences-200.tsv) records each of
the 159 matching lines in the audited revision, with a disposition and reason.
Line numbers refer to that revision. The search was:

```bash
rg -n 'ParanalNollSkyCalcFors1|airglow_cont\.dat|Paranal-derived|legacy|reference model|global-planning-model-unavailable|SkyCalc night bins|site airglow scale' --glob '!Cargo.lock' --glob '!*.svg'
```

There were no exact matches for the final two phrases; semantic night-bin and
scale dependencies were audited above. `legacy` also matches unrelated
Starlight fixtures and Moonlight/Zodiacal transport. Those are explicitly
classified for retention, not mechanically deleted. “Retain because
independently generic” in the inventory also covers these independently scoped
non-Airglow matches; it does not promote their scientific maturity.

Dispositions are **future migration instructions**, not claims of removal in
this Phase 1 change. Scientific history and third-party notices must remain
accurate. Parser/build integrity tests should be retargeted to admitted assets;
old-number equivalence tests should disappear. Geometry, units, malformed-input
checks and no-coordinate-implied-calibration contracts remain valuable.

Review also covered the Airglow guide, scientific metadata/maturity/validation
specifications, developer API policy, maintainer data pipeline, root/user
guides, manifest, and previous Airglow decision records. This change marks the
old policy as superseded and links the scientific stop; it does not rewrite
current-runtime descriptions as though migration had occurred.

`crates/nsb/api/API_FROZEN` is absent at this revision. The API README and
developer policy attribute that deliberate pre-freeze state to #185. No
public symbol or snapshot changes in this phase. A future implementation must
use `scripts/check-public-api.sh --write` after reviewing the smallest necessary
delta; it must not add speculative table types or a site-model enum.

Issue ownership remains #200 for source/admission/calibration separation,
#150 for dependency ownership, #151 for outcomes, #152/#153 for optimization
after representation selection, #154 for release audit, #38 for CTAO evidence,
and #187 for transport. No missing upstream primitive was established, so no
Siderust issue or local duplicate was created. No remote issues were changed.

## Requirements for resuming implementation

1. **Admit a scientifically complete source product.** Stage versioned optical
   inputs, verify redistribution rights and source checksums, document absolute
   calibration and sampling/quality cuts, and demonstrate spectral closure in
   300–650 nm. If the resulting model covers only components or a narrower
   domain, make that limitation explicit before changing the aggregate API.
2. **Reproduce fabrication.** Use a versioned `nsb-data-tools` generator,
   config, reaction/fit algorithm and input manifest. Record generator commit,
   source/output schema, DOI/release, units, coordinate/time conventions,
   checksums, applicability and a validation report. Repeat generation and
   compare bytes; reject malformed/nonfinite grids and incomplete provenance.
3. **Define predictors per component.** Justify geographic versus geomagnetic
   coordinates, date/local-solar-time definitions, solar proxy/averaging, layer
   distributions and activity exclusions. Reuse F10.7 infrastructure only if
   those conventions match. Missing support must fail explicitly, never clamp
   to plausible emission or select a site-trained spectrum.
4. **Validate on held-out observations.** Fit source data separately from
   independent site/instrument/year observations. Suitable leads include
   Paranal spectra, ORM/La Palma and Calar Alto sky measurements, and separately
   calibrated optical airglow networks. These are leads, not acquired data or
   passed validation. Broad-band total-sky photometry needs subtraction of
   celestial light, artificial light and other components, with propagated
   error, before it can constrain Airglow. See
   [Benn & Ellison](https://arxiv.org/abs/astro-ph/9909153),
   [Sánchez et al.](https://arxiv.org/abs/0709.0813), and
   [MANGO](https://doi.org/10.1029/2023JA031589).
5. **Quantify honest predictive uncertainty.** Separate measurement/retrieval
   error, model bias, natural exposure-to-exposure variability, parameter error,
   transport error and calibration residuals. Estimate component covariance
   where supported, propagate it through the band integral, and report
   empirical interval coverage on held-out data. Do not convert source-table
   scatter into a global accuracy percentage. Report missing uncertainty as
   missing. No numerical acceptance tolerance is fabricated in this record;
   predeclare tolerances from data uncertainties and planning needs before
   evaluating a final held-out set.
6. **Report portability and failure boundaries.** Publish sample counts,
   bias, MAE/RMSE, residual quantiles and interval coverage by site, component,
   wavelength, latitude/geomagnetic coordinates, zenith, season, local time
   and solar activity. Hold out whole sites and nights to avoid correlated
   leakage. Test sampling gaps and activity extremes; do not extrapolate
   silently. SkyCalc/H.E.S.S./nsb2 agreement is a separate cross-model test.

## Calibration design constraints, not a frozen schema

Keep generic model identity/version distinct from an optional external
calibration ID. A product must declare site/domain, scientific correction
quantity and pipeline stage, method, correction bytes, model compatibility,
wavelength/time/atmosphere bounds, held-out validation, residual uncertainty,
source rights, generator/config and checksum. Do not decide between component
amplitudes, shape, temporal response, layer or atmosphere corrections until
actual residual evidence identifies the quantity to correct.

Keep physical zero, unsupported generic-model domain, invalid generic asset,
no calibration selected, requested calibration unavailable, incompatible
calibration, calibration outside its domain, and invalid user input distinct.
None of the error states is zero radiance or a reason to switch source models.

Selection without calibration is normal generic evaluation; coordinates never
select calibration. An explicitly requested missing product, incompatible
model version, malformed asset or out-of-domain correction must fail distinctly.
Uncertainty must be recomputed using the calibrated quantity and measured
residuals/covariance, not copied or arbitrarily shrunk. No Paranal calibration
is available against an unselected generic model, and none is shipped.

Future tests must cover deterministic/typed units, spectral/domain boundaries,
all supported geographic/time/solar predictors, uncertainty and unsupported
outcomes; then no-calibration evaluation, valid calibration, incompatibility,
applicability, provenance, uncertainty update and coordinate independence.
Synthetic calibrations can test mechanics but cannot establish scientific
admission. There is no reason to invent a public transformation engine now.

## Delivered scope and verification

New generic assets: **none**. New calibration assets/schema: **none**. API delta:
**none**. Paranal production removal: **not performed**. Independent observations
processed and residuals measured: **none**. Replacement/calibration benchmarks:
**not applicable**, because no path was implemented. #200 remains incomplete.

The requested baseline workspace/API/coverage checks are recorded separately
in [verification](generic-model-verification-200.md). Software checks cannot
resolve the scientific admission gap, and passing existing regressions does
not validate a generic replacement.
