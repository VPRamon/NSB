# Starlight B/V candidate for issue #317

The bundled Starlight product contains only the 300–650 nm radiance and its
three uncertainties. In v1, zero B/V fields indicate absent diagnostics.
Issue #317 tracks a source-derived, top-of-atmosphere extension. The current
work stages a candidate. Scientific and redistribution admission are TBD.
[Issue #317](https://github.com/VPRamon/NSB/issues/317)

## Measurement and population

B and V denote vacuum wavelengths 445 and 551 nm. The worker accumulates
spectral energy flux density in W m⁻² nm⁻¹ per Galactic NESTED pixel. Packing
divides by the pixel solid angle and by the existing NSB calibration of
\(1.28\times10^{-11}\) W m⁻² sr⁻¹ nm⁻¹ per S10. The evaluator retains
\(27.78-2.5\log_{10}(\mathrm{S10})\) in mag arcsec⁻². The magnitude zero
point approximates \(10+2.5\log_{10}(3600^2)\). The shared energy normalization
is an NSB historical convention. Johnson filter responses and Johnson/Vega
zero points are outside this spectral contract. An energy-density input
already represents the energy carried at its wavelength. A photon-density
input would require multiplication by \(hc/\lambda\) before using the same
S10 calibration.

- **Gaia XP.** The pinned GaiaXPy v375wi design is interpolated at 444/446
  and 550/552 nm. Both interpolation brackets are exclusively BP. The
  resulting coefficient-space functionals propagate the full packed BP
  covariance, including the B–V cross term and the correlations between
  the samples used for interpolation. Signed source estimates are summed
  without clipping. A negative pixel total blocks packing. Statistical
  errors from distinct sources combine in covariance. A separately pinned
  absolute-calibration term remains shared. [Montegriffo et al. (2023)](https://doi.org/10.1051/0004-6361/202243880), [De Angeli et al. (2023)](https://doi.org/10.1051/0004-6361/202243680)
- **Gaia without continuous XP.** Learn the two energy-flux-density ratios
  \(R=F_\lambda/P_{336–650}\) from measured Gaia XP. The denominator is the
  existing NSB photon integral of the calibrated XP samples. A ratio has
  units J nm⁻¹ per integrated photon. At application, multiply both ratios
  by the estimated, unweighted 336–650 nm photon flux from the approved
  NSB photometric route. Its normalizer, UV correction, source selection
  and selection weight retain their existing semantics.
- **Photometric population prior.** The training anchors require positive
  3σ detections in both bands and in the photon integral, complete observed
  Gaia colours, and G < 17.65. The mixture uses a Galactic NESTED nside-1
  prior and magnitude groups split at G=14 and 16. Gaussian weights
  with bandwidths 0.26 mag in BP−G and 0.23 mag in G−RP condition on the
  observed colours. A missing colour contributes unit likelihood, which
  marginalizes that dimension across the XP population. G-only uses the
  magnitude- and sky-conditioned mixture without a colour likelihood.
  Local moments shrink toward the all-sky colour/magnitude moments using
  40 effective prior anchors. At sparsely sampled colours the all-G prior
  stabilizes the all-sky magnitude prior. Interpolation combines first and
  second moments so the predictive covariance includes between-template
  scatter. The held-out global shape is available as a comparator.
- **Transfer and uncertainties.** The conditional first two moments of
  \(F=PR\) use \(\mathrm{E}[P^2]\mathrm{Cov}(R)
  +\mathrm{Var}(P)\mathrm{E}[R]\mathrm{E}[R]^\top\) under conditional
  independence. Label noise and unresolved shape differences contribute
  to the empirical scatter. The schema-v3 artifact also carries a
  validation-derived coherent photon-normalization envelope. It takes
  the larger of the inherited photon systematic and the validation scale,
  and combines that result with local/global and alternate magnitude
  shape sensitivity in the shared inference group. Weak support adds a coherent transport scenario based
  on the empirical shape scatter, with multipliers of 1.5 for the
  missing-colour prior and 2 for sparse support or extrapolation.
  Edge-clamped colour and magnitude inference, sparse local XP support,
  and missing-colour priors are counted in the per-pixel evidence.
  The direct XP scale uses a 2% correlated sensitivity scenario motivated
  by the external residual scale reported above 400 nm. That scenario's
  coverage and spatial coherence require independent review.
- **Transport limitation.** XP selection and the high-signal anchor
  restriction favour bright, measurable spectra. The original Gaia
  comparison catalogue exposes this dependence on magnitude, latitude,
  reported blends and observed photometry. An XP holdout tests the
  bright population. Its masked-colour routes test sensitivity to
  absent photometry under the XP selection. The faint, crowded and
  naturally incomplete populations use approximate transported priors
  with broadened uncertainties and require independent validation.
- **Hipparcos supplement.** The same assigned CK04 template and Hp photon
  normalization yield B/V energy density with the existing Hipparcos
  identities and Gaia-replacement precedence. The Hp error is a shared scale
  across B and V for one source. Template/mapping uncertainty follows the
  existing source-independent normalization convention pending wavelength-
  specific validation. The catalogue Hp zero point remains correlated.
  See the [bright-star design and inputs](bright-stars/README.md).
- **Integrated admission and selection.** The existing 300–650 nm worker
  still decides which sources enter. It uses the same UV correction,
  catalogue exclusions, colours for selection, and capped inverse-
  completeness weights. An unusable continuous XP record can already be
  excluded by the combined worker's calibration or quality gates. B/V
  follows those admissions. It uses the selection weight once per admitted
  source and no weight for the separate supplement. B/V treats selection sigma as a
  shared envelope. The current integrated faint-tail fraction contributes to
  uncertainty and accounting only. The candidate preserves that scope. A
  missing B/V route or reused integrated-only shard blocks the B/V candidate.

GAMBONS is a precedent for a spatially resolved Gaia/Hipparcos stellar
photometry pipeline. Its native passbands and the definition of its stellar
component must be matched before quantitative comparison. Melchior et al.
estimate a Solar-neighbourhood spectrum with magnitude limits, latitude
weighting and volume corrections. A single such spectrum would remove
directional colour variation. [Masana et al. (2021)](https://arxiv.org/html/2101.01500),
[Melchior et al. (2007)](https://www.aanda.org/articles/aa/pdf/2007/06/aa6188-06.pdf)

## Photon-normalization audit

The replay used the same 989 XP sources, sky splits and pinned artifacts.
The approved full-route photometric prediction supplies 70.43% of the
measured XP photon sum. The median individual predicted/measured ratio is
0.739 and the source-weighted mean is 1.030. The brightest 1% by measured
photons have a sum ratio of 0.619. Photon-sum bias varies by colour,
magnitude and latitude. It is −24.2% at absolute latitude below 10° and
−37.7% between 30° and 60°. Gaia blend flags provide a crowding proxy.

The discrepancy enters through the pinned log-flux prediction. The
[checked-in XP-scale test targets](../../../crates/nsb-data-tools/tests/fixtures/photometric_xp_validation_v1.json)
for the full route follow the nominal analytic G/colour curve with
1.4 × 10⁴ photons m⁻² s⁻¹ at G=15 and BP−RP=0.8. Those tests
check the implementation against that curve. The artifact names an
external fitting script and contains a synthetic sky-split identifier.
The training spectra and selection are unavailable here, so the audit
cannot attribute the residual to a specific historical fitting choice.
The measured log residuals have a shifted center and a skewed tail.
A generic log-smearing factor would leave the colour- and
population-dependent discrepancy unexplained.

Both paths use the nominal unweighted 336–650 nm photon band. NSB
trapezoid-integrates the monochromatic energy density times wavelength
divided by hc, with wavelength in metres in the conversion and bin width
in nm. A separate trapezoid of 100 archive sampled XP spectra agreed with
the Rust continuous integral to 6.4 × 10⁻⁶ maximum relative difference.
This cross-check shares Gaia XP's instrument. The worker multiplies the
shape by the raw photon prediction and then applies the admitted source
weight once. The UV correction enters the legacy integrated flux only.
G+BP uses the integrated G-only normalizer because the pinned partial
route requires RP. Its B/V shape still uses BP.

Let X be the measured XP photon integral, P the photometric prediction
and R the predicted monochromatic flux per integrated photon. The
monochromatic residual splits into the shape error at measured photon
flux, X × R − F(XP), and the normalization contribution, (P − X) × R.
On the full route the shape terms contribute −1.67 and +1.40 percentage
points in B and V. The normalization terms add −32.15 and −29.25 points.
The paired signed terms correlate in this XP sample. The conditional
stochastic covariance of future non-XP errors remains unconstrained.

The validation-only, route-and-magnitude scale envelope takes the
absolute flux-sum discrepancy relative to predicted photons plus its
delete-region jackknife standard error, using Galactic nside-1 blocks.
It requires 32 sources spanning at least five nside-16 cells and two
nside-1 blocks. A sparse magnitude cell uses its pooled route result.
Fitting stays below G=17.65, and extrapolation uses the largest supported
envelope for the route. This is a coherent sensitivity scale. It takes
the maximum of the new estimate and the inherited photon systematic,
which describe the same scale, and adds shape transport linearly within
the common `gaia-bv-inference` group. Point predictions and independent
statistical covariance are preserved. The latter uses the existing
conditional product-moment approximation. Direct XP monochromatic
covariance still projects the packed BP coefficient covariance. The
reported photon-integral error uses grid-diagonal propagation and serves
as a quality proxy; joint reference noise is retained in the empirical
shape scatter.

| XP test | Route | Photon-sum bias | B/V flux-sum bias | B/V 68% coverage before → after | B/V 95% coverage before → after |
| --- | --- | ---: | ---: | --- | --- |
| Original 989 | Full | −29.6% | −33.8% / −27.8% | 54.5/37.5 → 65.6/62.4 | 86.9/95.9 → 95.7/99.9 |
| Original 989 | Masked G+BP | −32.4% | −37.6% / −30.5% | 61.2/38.5 → 72.9/66.9 | 90.0/93.0 → 97.9/99.8 |
| Original 989 | Masked G+RP | −38.4% | −41.8% / −36.8% | 57.4/37.0 → 80.0/94.5 | 95.2/99.2 → 99.0/100 |
| Original 989 | Masked G-only | −32.4% | −37.7% / −30.2% | 55.5/41.2 → 69.6/71.6 | 91.9/97.8 → 98.0/100 |
| New 237 | Full | −32.0% | −35.0% / −29.8% | 60.3/37.6 → 69.2/62.0 | 88.2/94.1 → 97.5/99.6 |

The new cohort uses Gaia DR3 random indices 45,000–56,999, previously
unexamined source IDs and the reserved test sky cells. It was scored after
freezing the validation rule and model. Both cohorts share Gaia XP's
instrument and selection. The percentages are per-source coverage against
measured XP at one or 1.96 times the marginal prediction scale. Some
routes overcover. The figures express marginal sensitivity coverage.
Pixel-level significance needs independent spatial calibration. The JSON report carries MAE, RMSE,
relative-error quantiles, stratified bias decomposition and nside-16
held-out pixel sums. Only two original test sources reached G ≥ 17.65.
The genuine pilot exercises natural incompleteness and faint fallback
for execution. Independent observations of the non-XP population remain
necessary to justify a point-scale correction and scientific approval.

## Compact layout and regeneration

The unpublished `NSBSTL02` layout keeps the v1 framing and SHA-256 trailer.
Each pixel has six little-endian binary64 values. The first four are
integrated photon radiance, statistical, systematic and total uncertainty in
the original units. The last two are B and V S10. The header declares
`s10_diagnostics=monochromatic_445_551_nm_nsb_s10`. An empty pixel has
available zero B/V flux. The immutable map exposes the existing evaluator
fields. For a supplied zero, the existing magnitude conversion uses its
historical positive floor. Runtime v1 loading still reports B/V unavailable.

The generation workflow writes one research bundle. The compact, uncertainties
and manifest are linked by SHA-256 to the integrated canonical map and merge
report. The NESTED uncertainty evidence records band fluxes, statistical and
independent systematic covariance triplets, shared group sigmas and route
counts and photometric inference support. An integrated-only shard cannot satisfy a B/V run. Candidate integrity
can pass ordinary dataset validation. Dataset publication and the v1 production
loader cannot admit the candidate.

## Scientific validation and production regeneration handoff

Use a clean checkout of the reviewed commit on Ladon with Python
3.11 or later, Cargo and the Slurm client tools. Keep the workspace, extracts, models and references outside
the checkout. Record the commit, toolchain, commands and input and output
SHA-256 digests.

- **TBD** — Representative non-XP calibration and independent evaluation
  of all four photometry routes.
- **TBD** — Independent CK04 B/V validation and review of uncertainty
  correlations and flux conventions.
- **TBD** — Full regeneration of 3,386 Gaia partitions and comparison of all
  four integrated fields and population counts against the approved product.
- **TBD** — Independent full-sky B/V validation, including regional accuracy,
  uncertainty coverage and spatial effects.
- **TBD** — Scientific admission and redistribution approval for the exact
  final checksums.

### Input inventory

| Input | Expected bytes and identity |
| --- | --- |
| Gaia source and XP inventories and objects | ESA Gaia DR3 bulk `GaiaSource_*.csv.gz` and `XpContinuousMeanSpectrum_*.csv.gz`, 3,386 matched partitions each. The checked-in [combined configuration](../../../crates/nsb-data-tools/config/starlight-production-300-650.ladon.toml) pins the SHA-256 of both upstream MD5 lists. Workers verify each listed download and the content-addressed cache receipt. Keep the catalogue headers, astrometry, population flags and source IDs intact. |
| Gaia XP design | Checked-in [GaiaXPy design JSON](../../../crates/nsb-data-tools/tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json), BP v375wi and RP v142r, SHA-256 `e8dc899916ed29d4a92ee6ff7ec3d88a85f9c26d9c7e375ef6a6b3dd76666539`. |
| UV correction | Validated schema-v1 JSON for 300–336 nm. The combined configuration pins SHA-256 `5918a7960bf7b0eec5abee77987d66ef106940cc590434fc2805c82d7d602367`. Obtain the original external bytes and accompanying calibration and reference receipts from the calibration workspace. |
| Integrated photometric inference | Validated XP-anchored schema-v1 JSON with branch and coefficient evidence. The combined config pins SHA-256 `02a6e5c98458351fb13ec7623cffa019a760bdf2e68cca64b80f9c5d7fe4f4f2`, available as an [identical checked-in calibration](../../../crates/nsb-data-tools/tests/fixtures/photometric_xp_anchored_v1/artifact.json). |
| Selection function | Validated schema-v1 JSON with Cantat-Gaudin/Gaia DR3 completeness and faint-tail metadata, SHA-256 `1a3670b56eedaf9f9de0b32f081ccfa2baf741a449cd70c2be37d666101a9711`. Obtain the external bytes and their calibration evidence. |
| Bright-star supplement | Use the checked-in [combined bright-star configuration](../../../crates/nsb-data-tools/config/starlight-bright-stars-combined-v1.ladon.toml) for exact SHA-256, release, format and retrieval identities. It pins external Hipparcos-2 and Tycho-2 fixed-width catalogue segments, XHIP fixed-width spectral types, the official Gaia identity crossmatch and the Gaia quality/positional CSV extracts, plus the CK04 template/assignment JSON, CK04 atlas archive, SVO Hp response and CALSPEC Vega inputs. ADQL recipes and mapping/uncertainty JSON are in Git. See the [bright-star input and reacquisition record](bright-stars/README.md) and [upstream terms](licensing/REACQUISITION.md). The approved supplement checksum is `dda54aa31544672d8c5dae3601b2c62a89f0a3499ebb2c937cef0f333d126017`; the rebuilt B/V-bearing supplement has its own checksum. |
| Non-XP B/V shape model | Generated in the candidate workspace from verified Gaia pairs and the compiled XP design. The schema-v2 research artifact pins the approved photometric normalizer, its source references and internally held-out report. |
| Approved integrated baseline | Checked-in [compact v1](../../../crates/nsb/data/starlight_nside128.compact.release.bin) with SHA-256 `11479bd0b6838d06df884b76dd74575bbc42d424ec85defecf847257c0f64d7f`. Use its [manifest](../../../crates/nsb/data/starlight_nside128.compact.manifest.toml), [baseline validation](production-runs/combined-300-650-validation.json), and original workspace merge report for the integrated policy and population differential. |

### Preflight and run on Ladon

From the root of the PR checkout, verify the Ladon partition, cache
identities and pinned auxiliary inputs:

```bash
python3 crates/nsb-data-tools/tools/regenerate_starlight_bv_ladon.py --check
```

Run the same entry point without `--check` to prepare and launch the
candidate, wait for workers, validate and merge, and check the integrated
differential:

```bash
python3 crates/nsb-data-tools/tools/regenerate_starlight_bv_ladon.py
```

Preflight checks the configured account, partition and resource request
with Slurm's `sbatch --test-only`. It reports shared-filesystem free space
and tests that the workspace root is writable. Each worker requests 32 GiB,
with at most eight tasks active per array. The local merge loads the
reconciled shards on the invoking node. Run finalization with sufficient
memory and scratch space under Ladon's login or compute policy.

The driver uses the checked-in combined-band configuration and expects the
original production tree at `/mnt/beegfs/valles/nsb-data/`. It looks first
for the configured `starlight-production-300-650-fix116-photometric` cache,
then for paired inventories under the data root. Supply `--cache` for a
cache in a different location, or `--data-root` if the shared tree has moved.
Pinned UV, photometric, selection and bright-star file locations are
resolved against the current tree and verified by SHA-256. Checksum or
availability failures print the exact input that needs recovery. Only the
two small upstream Gaia MD5 manifests need network access. The driver
checks both against the production pins, verifies every paired receipt
against those inventories and its object sizes, then copies inventories
and receipts into the new workspace. Training verifies the selected
compressed objects by MD5 and SHA-256. Every worker verifies its object's
SHA-256 before reading it. The original cache receives read traffic.

The driver takes a fixed random-index sample from 24 evenly spaced
partition ranges, exports true coefficient-space B/V and measured photon
integrals, builds the schema-v3 shape artifact and scores held-out sky
cells. Training, validation and test are assigned by the existing
Galactic nside-16 cell hash. The fit stores a table of smooth
colour-conditioned moments for every context and observed route.
There is no manual preparation of `labels.csv` or `policy.json`.
The previous externally reviewed schema-v1 workflow remains available
for experiments via `prepare_starlight_bv.py`; it is superseded for
launching this candidate.

The workspace is printed at launch. Its generated `combined-bv.toml` pins
the fitted model and the B/V-enabled Hipparcos/CK04 supplement. The driver
submits all 3,386 Gaia partition pairs through `nsb-data`. Pass
`--submit-only` to return once arrays have been submitted. Closing SSH
after submission leaves the Slurm arrays running. If SSH disconnects
during preparation or submission, the printed workspace lets you recover
completed steps and accepted job IDs. After a disconnect, failed array,
or submission-only run, pass the printed workspace
to the same entry point:

```bash
python3 crates/nsb-data-tools/tools/regenerate_starlight_bv_ladon.py \
  --resume /mnt/beegfs/valles/nsb-data/starlight-bv-candidate-PRINTED-WORKSPACE
```

The driver retains completed preparation, immutable per-run array manifests,
and each accepted job ID. It submits from the shared `map/` directory, where
Slurm writes its job logs. Array workers use the binary and XP design snapshot
in the shared candidate workspace. The driver waits for live arrays before
asking the run engine to retry incomplete work. If a retry still leaves missing
partitions, inspect the Slurm logs before resuming. Keep the same checkout,
cache and candidate workspace for recovery. A changed input pin, software
commit or fit creates a fresh candidate workspace.

Expected outputs under that workspace:

| Output | Location |
| --- | --- |
| Shape artifact and spatial holdout | `fit/bv-shape.json` and `fit/shape-held-out.json` |
| Combined bright-star artifact and checksum evidence | `bright-stars/` |
| Combined configuration and Slurm records | `combined-bv.toml` and `map/runs/starlight/` |
| Merged 300–650 nm map and ordinary validation | `map/outputs/` and `map/validation.json` |
| Compact v2 and route, support and uncertainty evidence | `map/bv-candidate/` |
| Verified integrated differential and input/output digests | `candidate-evidence.json` |

The final evidence is written only after all four integrated fields agree
bit-for-bit with the approved v1 across 196,608 RING pixels and population
accounting matches the approved merge report. This is technical admission
for a separate research candidate. The publication and runtime production
gates retain their existing checks.

### Evidence from the small pilot

The development pilot used genuine Gaia DR3 continuous XP and natural
full, G+BP, G+RP and G-only observations from verified bulk pair
`466830-466850`, plus the approved integrated photon estimator. Its UV,
selection and bright-star/replacement inputs were integration fixtures
because the pinned Ladon bytes were unavailable. It exercised actual
XP projection, the non-XP worker, selection, CK04 and replacement
mechanics, compact v2 and the public evaluator. The integrated pilot
CSV was identical with B/V enabled and disabled. The ordinary full-sky
plane-coverage gate rejected the one-partition footprint and compact
candidate integrity passed. The normalization revision preserved the
pilot's B/V means and random covariance and broadened its shared
inference envelope. The pilot supplies engineering evidence;
it cannot establish full-sky bitwise parity or calibrate transport to
faint and naturally incomplete populations.

### Score an independent full-sky reference

Supply a checksum-pinned JSON reference with this contract. The `pixels`
array is dense and indexed by Galactic NESTED pixel (196,608 cells at nside
128). Each entry contains `flux: [B,V]` and `sigma: [B,V]` in W m⁻² nm⁻¹
integrated over that cell, with positive finite fluxes and non-negative
finite standard uncertainties. Convert spectral radiance to per-cell flux
with the exact cell solid angle; remap/regrid a different reference with a
documented flux-conserving operator. The top-level keys are:

```text
schema        nsb-starlight-bv-reference-v1
source        reference name, release and scientific citation
upstream_sha256  array of input SHA-256 digests
operator      extraction, spectral, spatial and population-matching procedure
nside         128
ordering      nested
frame         galactic
domain        top-of-atmosphere-stellar-only
reference_nm  [445, 551]
unit          W m-2 nm-1
pixels        array of {"flux":[B,V],"sigma":[B,V]} entries
```

```bash
# Set HANDOFF to the candidate workspace printed by the Ladon driver.
WORKSPACE="$HANDOFF/map"
BUNDLE="$WORKSPACE/bv-candidate"
NSB_DATA="$HANDOFF/bin/nsb-data"
sha() { sha256sum "$1" | cut -d ' ' -f 1; }
REGIONS="$PWD/docs/nsb_components/starlight/validation/regions-v1.json"
REFERENCE="$HANDOFF/independent-bv-reference.json"
"$NSB_DATA" dataset starlight validation bv \
  --workspace "$WORKSPACE" --manifest-sha256 "$(sha "$BUNDLE/manifest.json")" \
  --reference "$REFERENCE" --reference-sha256 "$(sha "$REFERENCE")" \
  --regions "$REGIONS" --regions-sha256 "$(sha "$REGIONS")" \
  --output "$HANDOFF/independent-bv-validation.json"
```

The command verifies the bundle and reports each band and region, including
bias, error percentiles, outliers and interval coverage. It applies the
existing NSB gates to each band (3% absolute all-sky bias, 5% median / 10%
P95 regional absolute relative error, and 63–73% / 90–98% interval coverage).
It combines per-pixel candidate and independent reference uncertainty in
quadrature and retains shared groups in the evidence for spatial analyses.
It exits unsuccessfully on a missed numerical gate after writing the report.
Review spatially correlated errors and seam/resolution comparisons with
the [NSB requirements](science-requirements.md#pre-registered-validation-gates).
Record decisions, including any new spectral-reference or source-level
tolerance, before inspecting test results.

Use genuinely independent spectra or a justified monochromatic transformation
with propagated uncertainty. Broad-band Johnson B/V, uncorrected ground-sky
photometry, a Gaia-XP-derived full-sky reference sharing the production
calibration, and a fixed spectrum over the sky cannot by themselves validate
these absolute reference estimates. Verify flux operators, admitted sources,
Gaia completeness limits, extinction/atmospheric domain and zero points.
Review the centre, plane, poles, high latitudes, bright-star dominated
regions and fields with contrasting colours.

### Review and recovery

- Inspect inference-support counts and the shape holdout before interpreting
  weak regions. Invalid shape inputs identify the admitted Gaia source and
  stop its worker. Preserve its original input receipt, diagnose support
  and regenerate a reviewed model in a new workspace.
- A checksum or schema failure requires re-verifying bytes from the pinned
  upstream receipt. Keep each calibration attempt's report and model paths
  distinct. A failed export can leave an older file at a reused path.
- For a missing shard, inspect the submitted run and the worker manifest
  before resuming. For inconsistent products, rerun the technical validation
  with verified inputs and inspect the named gate and merge report. Preserve
  the failed scientific report and resolve the discrepancy before a reviewed
  rerun. Do not edit generated maps or manifests.
- Archive the frozen policies, label and reference receipts, held-out and
  CK04 reports, worker/build/validation manifests, exact integrated
  differential, regional comparison and seam/resolution evidence, and bundle
  and software digests. The existing [approval inventory](licensing/approval-inventory-v1.toml)
  governs the old bytes. A future registry update needs separate scientific
  and redistribution approval for the exact new bytes. Use the immutable
  v2 map through `StarlightMap::from_compact_bytes` for candidate research.

## Local validation evidence

The [local smoke receipt](validation/results/bv-photometry-317-smoke.json)
records sample and output identities, all six sky regions, held-out scores
and both partition comparisons.

- **XP parity.** The Gaia Archive DataLink sample comprised 144 sources with
  G between 10 and 15, 24 per region. The same linear midpoint operator was
  applied to the archive's externally calibrated sampled spectra. The
  pre-execution smoke thresholds were 1% median and 3% 95th percentile
  absolute relative error per wavelength. Measured P95 errors were
  \(1.75\times10^{-6}\) and \(4.08\times10^{-8}\) as fractions in B and
  V. The median source B/V energy-density ratio was 0.336 in the centre
  sample and 0.947/0.960 in the polar samples. These pairs share Gaia
  calibration and test reconstruction parity only.
- **Non-XP inference.** The XP sample split 80/42/22 by source and 63/34/18
  by sky cell. All 144 targets had natural full colour. The preset
  minimum-support checks admitted zero natural-availability cells. Under
  artificial masking, four cells fit and 10 of 88 test route entries
  were covered. In one G-only cell, median absolute relative errors
  were 38.4% (B) and 32.1% (V) over five test entries. These results
  are insufficient to export a calibration or estimate population-level
  uncertainty coverage.
- **Map controls.** Two official Gaia partition pairs selected from the
  checked-in 48-partition list completed the XP-only 336–650 nm worker
  on both latest main and this branch. The 51,773 admitted sources,
  exclusions, exact sums, and serialized shards matched byte for byte.
  A combined-band synthetic fixture preserves the entire integrated
  canonical CSV with B/V enabled, verifies B/V flux conservation,
  deterministic merge, true zero and checksum-verified compact loading.
  A representation-only control matched all four approved integrated fields
  across 196,608 pixels and rejected an altered uncertainty field.
  **TBD** — The full 300–650 nm production differential requires the original
  pins and a representative B/V model.
- **Full-sky science (TBD).** Use the preregistered [NSB regions and
  thresholds](science-requirements.md#pre-registered-validation-gates)
  independently in B and V for compatible stellar-only,
  top-of-atmosphere reference operators. Include the centre, the plane,
  poles, high latitude, bright-star fields, and contrasting colours.
  Compare wavelength-matched calibrated spectra of the represented stellar
  populations, or perform a justified spectral transformation with its
  uncertainty. Require the existing 3% all-sky bias, 5% median/10% P95
  regional absolute-error and interval-coverage gates, along with source
  accounting, seam checks and a complete integrated differential.
- **Ladon and admission.** BeeGFS and Slurm were unavailable locally, and
  the `ladon` SSH attempt timed out. The Gaia inventories and two smoke
  partitions were acquired from ESA, and the exact integrated photometric
  pin and XP design are available in the checkout. The pinned production
  UV and selection artifacts and the original combined bright-star
  workspace remain external. The checked-in v1 default remains the approved
  runtime asset. See the
  [approval inventory](licensing/approval-inventory-v1.toml).

### Smoke input selection

Use the public ESA Gaia DR3 TAP endpoint. Select
`source_id,ra,dec,l,b,phot_g_mean_mag,phot_bp_mean_mag,phot_rp_mean_mag,bp_rp`
from `gaiadr3.gaia_source` with `SELECT TOP 24`,
`has_xp_sampled='true'`, `phot_g_mean_mag BETWEEN 10 AND 15`
and `ORDER BY random_index`. For the five non-centre fields apply
`MOD(random_index,1009)=23` and one of these cuts.

| Sample | Galactic cut in degrees |
| --- | --- |
| Plane | `l BETWEEN 75 AND 105 AND ABS(b)<10` |
| North pole | `b>65` |
| South pole | `b<-65` |
| Northern high latitude | `b BETWEEN 35 AND 55 AND l BETWEEN 165 AND 195` |
| Southern high latitude | `b BETWEEN -55 AND -35 AND l BETWEEN 255 AND 285` |

The centre field used `random_index BETWEEN 0 AND 100000` and an ICRS
circle of radius 12° centred on RA 266.41683°, Dec −29.00781°. Retrieve
matched XP_CONTINUOUS and XP_SAMPLED from Gaia DataLink with `RELEASE=Gaia DR3`,
`FORMAT=csv`, and `DATA_STRUCTURE=DATAMODEL_GAIA`. The sampled array is
the 336–1020 nm grid in 2 nm steps. Average indices 54/55 and 107/108
(zero-based), and match by source ID before scoring. The real-worker
control used `starlight-production.toml` with a temporary local workspace,
`--partitions 484790-484820,466830-466850 --concurrency 2`, and the
ordinary update and build commands. [ESA programmatic access](https://www.cosmos.esa.int/web/gaia-users/archive/programmatic-access)
