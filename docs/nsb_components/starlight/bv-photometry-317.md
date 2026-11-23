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
- **Gaia without usable XP.** Conditional means and predictive covariance are
  calibrated in linear flux density normalized to G=15. Coarse sky and G
  bins are common to all routes. Full colour uses BP−G and G−RP. Each partial
  route uses its one observed colour, and G-only uses the sky- and
  magnitude-conditioned population distribution. This estimates a population
  mean and retains conditional scatter. Within-cell held-out residuals give
  a separate correlated uncertainty envelope. There is no automatic
  extrapolation or colour substitution. Production needs representative
  examples of naturally incomplete photometry, independent checks of faint
  and crowded sources, and coverage of every admitted source. Masking
  photometry on an XP sample tests information loss under artificial
  missingness.
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
counts. An integrated-only shard cannot satisfy a B/V run. Candidate integrity
can pass ordinary dataset validation. Dataset publication and the v1 production
loader cannot admit the candidate.

## Scientific validation and production regeneration handoff

Use a clean checkout of the reviewed commit on the data host with Python
3.10 or later. Keep the workspace, extracts, models and references outside
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
| Non-XP B/V calibration | External matched reference CSV, frozen policy JSON, independent holdout report and candidate calibration JSON generated below. The model carries policy and report digests, split identities and support counts. Archive checksums of all four files along with the parent extracts, software commit, query and selection/matching operators. |
| Approved integrated baseline | Checked-in [compact v1](../../../crates/nsb/data/starlight_nside128.compact.release.bin) with SHA-256 `11479bd0b6838d06df884b76dd74575bbc42d424ec85defecf847257c0f64d7f`. Use its [manifest](../../../crates/nsb/data/starlight_nside128.compact.manifest.toml), [baseline validation](production-runs/combined-300-650-validation.json), and original workspace merge report for the integrated policy and population differential. |

### 1. Freeze and calibrate the source model

The handoff workflow needs Python 3.11 or later. Run this block in a
checkout of PR #319 on Ladon. It selects up to six evenly spaced entries in
the sorted list of paired receipts. Set `CACHE` explicitly if the original
workspace is elsewhere. A small or geographically clustered cache gives a
local diagnostic. The procedure verifies both pinned ESA MD5 inventories
and each selected compressed object against its official MD5 and receipt
SHA-256. Only the small MD5 lists are fetched. It reads the original cache
and writes all derived files to a fresh workspace.

```bash
set -euo pipefail
REPO="$PWD"
ROOT=/mnt/beegfs/valles/nsb-data
CACHE="${CACHE:-$ROOT/starlight-production-300-650-fix116-photometric}"
for product in gaia-source xp-continuous; do
  test -d "$CACHE/cache/receipts/$product" || {
    printf 'Missing %s receipts under %s\n' "$product" "$CACHE" >&2
    find "$ROOT" -type d -path "*/cache/receipts/$product" -print
    exit 1
  }
done
PARTS="$(python3 - "$CACHE" <<'PYCODE'
from pathlib import Path
import sys

root = Path(sys.argv[1], "cache/receipts")
gaia = {p.stem for p in (root / "gaia-source").glob("*.json")}
xp = {p.stem for p in (root / "xp-continuous").glob("*.json")}
paired = sorted(gaia & xp)
count = min(6, len(paired))
if not count:
    raise SystemExit("No paired GaiaSource and continuous-XP receipts")
for i in range(count):
    print(paired[(2*i + 1)*len(paired) // (2*count)])
PYCODE
)"
PART_ARGS=()
while read -r part; do PART_ARGS+=(--partition "$part"); done <<< "$PARTS"
HANDOFF="$(mktemp -d "$ROOT/starlight-bv-317.XXXXXXXX")"
WORKSPACE="$HANDOFF/map"
CARGO_TARGET_DIR="$REPO/target" cargo build --locked -p nsb-data-tools --release --bin nsb-data
NSB_DATA="$REPO/target/release/nsb-data"
COMMIT="$(git rev-parse HEAD)"
sha() { sha256sum "$1" | cut -d ' ' -f 1; }

python3 crates/nsb-data-tools/tools/prepare_starlight_bv.py \
  --out "$HANDOFF" --nsb-data "$NSB_DATA" --cache "$CACHE" "${PART_ARGS[@]}"
(cd "$HANDOFF" && sha256sum -c checksums.sha256)
printf 'Diagnostic handoff %s\n' "$HANDOFF"
```

The same command can use `--public` in place of the cache and partition
options. That mode selects the first 45,000 published Gaia DR3
`random_index` values via ESA TAP and requests exactly the eligible XP IDs
from the Heidelberg Gaia TAP mirror in reproducible batches.
`--random-index-limit` changes the comparison cut. Cached partitions use
the same cut, with a default of 15 million to give a useful sample from a
small set of partitions. A smaller cut reduces memory and computation.
Completed public batches, their ADQL, the assembled input digests and the
frozen policy are retained. Rerun the same command on the same output
directory and same binary to resume. Use a fresh directory for changed
inputs. `--xp-extract /path/extract.json` is the offline option for existing
matched plain CSVs. The manifest contains `origin` (the release, source query
and selection) and `gaia_source` and `xp_continuous` entries with `path` and
`sha256`. Relative paths are resolved from the manifest.

`labels.csv` has `source_id,galactic_nest16,g,bp,rp,b_w_m2_nm,v_w_m2_nm`.
The Rust join computes the Galactic index from Gaia DR3 ICRS astrometry
and rejects mismatched or duplicate IDs. Photometry blanks preserve
catalogue missingness. `label-covariance.csv` preserves the signed B–V
statistical covariance and both variances in squared flux-density units.
The coefficient-space fluxes are W m⁻² nm⁻¹ at 445 and 551 nm in vacuum.
The same files feed the existing calibrator, which assigns whole Galactic
nside-16 pixels to disjoint training, validation and test roles. Label
covariance is audited and the fitted residual scatter retains label noise.
`provenance.json` also counts the comparison catalogue by natural route,
XP availability, magnitude, Galactic latitude and reported BP/RP blends.
These catalogue proxies describe crowding in the comparison sample.
The integrated worker applies further admission criteria.

The XP diagnostic uses a coarse nside-1 grid, magnitude cuts at G=15
(the public sampled-XP boundary) and 17.65 (the main continuous-XP limit),
and broad colour strata around BP−G=0.60 and G−RP=0.85. These cuts group
blue and red sources without fitting boundaries to holdout residuals.
The outer bounds flag unusual catalogue colours and magnitudes. The
25-training, 10-validation and 5-validation-cell minima screen out the
most unstable cells while allowing a diagnostic sample to fit. Even 25
independent Gaussian observations give a relative standard error of about
29% on sample variance. Five sky units give a noisy validation envelope.
Those minima and nside are unsuitable as production acceptance criteria.

The previous 144-source check was same-source XP parity, and its natural
routes fitted zero cells. The new workflow uses natural availability and
reports unsupported routes as missing. XP selection favours brighter
sources with sufficient transits in both spectrometers, so a sky-disjoint
XP holdout still cannot demonstrate transfer to crowded and faint non-XP
stars. Montegriffo et al. report wavelength-dependent external residuals
typically around 2% above 400 nm for standards. That measurement combines
multiple error sources and does not identify the scale component shared
by the map's XP sources at both bands. The diagnostic therefore leaves
`xp_shared_calibration_fraction` absent. A candidate requires independent
absolute standards and a reviewed derivation of the shared B and V terms.
[XP processing](https://doi.org/10.1051/0004-6361/202243680)
[XP external calibration](https://doi.org/10.1051/0004-6361/202243880)

Public CALSPEC standards can test the absolute scale, subject to checking
which stars were used to fit XP and to crossmatch quality. Ordinary
LAMOST relative spectra require independent absolute scaling, calibrated
response and fibre-loss/contamination checks. Nominal MUSE globular-cluster
spectra start at about 475 nm and can only test V here. Neither route
by itself establishes the four-route population calibration. The old UV,
integrated inference and bright-star artifacts cover distinct measurements.
[STScI CALSPEC](https://www.stsci.edu/hst/instrumentation/reference-data-for-calibration-and-tools/astronomical-catalogs/calspec)
[LAMOST flux calibration](https://doi.org/10.1093/mnras/stu2434)
[MUSE spectral coverage](https://doi.org/10.1093/mnras/stx2719)

#### Fit a candidate after the independent inputs are available

Use the same script with `--out "$HANDOFF/independent" --nsb-data "$NSB_DATA"
--review "$ROOT/starlight-bv-references/review.json"`. The review manifest
pins the following files with `{"path": "...", "sha256": "..."}` entries.
Generate the crossmatch from a published Gaia ID link or a reviewed
position/epoch crossmatch with ambiguous/blended matches removed. Preserve
observed Gaia photometry and the crossmatch ledger. References must be
absolutely calibrated observed-frame vacuum spectra in the same energy
flux convention. For example, spectral density in erg s⁻¹ cm⁻² Å⁻¹ is
multiplied by 0.01 for W m⁻² nm⁻¹. Convert wavelengths and density together
if converting from air, and evaluate both bands with a documented
sampling, resolution and covariance operator.

| Manifest entry | Required input |
| --- | --- |
| `gaia_source` | Plain Gaia CSV with `source_id,ra,dec,phot_g_mean_mag,phot_bp_mean_mag,phot_rp_mean_mag`. Exactly the externally matched IDs. |
| `spectral_reference` | Plain CSV with `source_id,b_w_m2_nm,v_w_m2_nm,stat_b_var,stat_v_var,stat_cross`. Both signed spectral labels and their statistical covariance. |
| `reference_evidence` | Reference release, upstream input hashes, absolute scale, quality criteria, ID crossmatch and flux extraction ledger. |
| `xp_absolute_calibration_evidence` | Independent standard-star comparison, per-band shared uncertainty derivation and treatment of correlated and individual noise. |
| `target_population_evidence` | Reviewed admitted-population coverage including faint and crowded sources and all natural routes. JSON with a `required_cells` array of objects with `route,galactic_nested_pixel,g_bin,bp_g_bin,g_rp_bin` (unused colour indices are null). This evidence requires the actual integrated-worker admissions. |

The manifest also contains `measurement` equal to
`{"vacuum_nm":[445,551],"unit":"W m-2 nm-1"}`,
`reference_independent_of_gaia_xp_and_photometry: true`, and nonempty
`selection_and_crossmatch`, `wavelength_operator`, `uncertainty_model`
and `reviewer` strings. Its `policy` object specifies `seed`, `sky_nside`
(1, 2, 4 or 8), the three increasing edge arrays, `min_training`,
`min_validation`, `min_validation_sky_cells` and the two positive
`xp_shared_calibration_fraction` values in B, V order.
Its `acceptance` object specifies `min_test_per_cell`,
`max_abs_bias`, `max_p95_absolute_relative_error`, `min_coverage_68`
and `min_coverage_95`. Freeze these choices and the reviewed target list
before reading the held-out test. Justify support by effective independent
sky cells, noise, crowding and flux contribution. The workflow supplies the
actual software/design pins and review digest, compares the tested
population with the predeclared target domain, and preserves the report on
scientific rejection. On a pass it prints the absolute path and SHA-256 for
`[starlight.bv_photometry]`. That artifact is eligible for candidate map
regeneration and still requires the gates below. A diagnostic run produces
no artifact pin. Before a candidate map run also verify the existing
integrated and bright-star inputs against their checked-in configuration pins.

### 2. Build and validate the bright-star supplement

Copy the pinned bright-star configuration to `$HANDOFF/bright-stars.toml`.
Set the top-level `include_bv_photometry = true` before the first TOML
section. Resolve paths to the original verified input bytes and leave their
digests, physical model, assignments and policies fixed. Invoke from the
repository root so the checked-in query and model paths resolve.

```bash
"$NSB_DATA" dataset starlight bright-stars build \
  --config "$HANDOFF/bright-stars.toml" \
  --config-sha256 "$(sha "$HANDOFF/bright-stars.toml")" \
  --expected-build-commit "$COMMIT" \
  --output-directory "$HANDOFF/bright-stars"
BRIGHT="$HANDOFF/bright-stars/starlight-bright-stars-v1.json"
```

Archive the generated build manifest, source classifications/diagnostics,
and artifact digest. Compare the supplemental and replacement identities
and integrated per-source fields with the original supplement. Review
unsupported template assignments, ambiguous/rejected matches, and uncertainty
groups. Validate Hipparcos/XHIP-selected CK04 at both wavelengths using
independent calibrated stellar spectra in the same monochromatic convention,
including colour and spectral-type trends and bright-star-dominated fields.
The Hp-zero-point group is common to both bands and to all supplement stars.

### 3. Regenerate and compare

Copy the pinned combined Ladon config to `$HANDOFF/combined-bv.toml`. Set
`[workspace].root` to the new absolute `$WORKSPACE`, resolve the three existing
calibration paths, and retain their SHA-256 values. Remove the optional
`[publish]` table. Set the two new pins using the literal absolute paths and
digests printed below. Leave the combined band, `nside = 128`, Gaia inventories
and source policy unchanged.

```bash
printf '[starlight.bright_star_supplement]\nartifact_path = "%s"\nsha256 = "%s"\n\n' \
  "$BRIGHT" "$(sha "$BRIGHT")"
printf '[starlight.bv_photometry]\nartifact_path = "%s"\nsha256 = "%s"\n' \
  "$HANDOFF/independent/bv-calibration.json" "$(sha "$HANDOFF/independent/bv-calibration.json")"

CFG="$HANDOFF/combined-bv.toml"
"$NSB_DATA" dataset starlight update --config "$CFG" --executor local
BUILD_JOBS="$("$NSB_DATA" dataset starlight build --config "$CFG" --executor slurm)"
find_build_run() {
  python3 - "$WORKSPACE" "$1" <<'PYCODE'
import json
from pathlib import Path
import sys

workspace, jobs = sys.argv[1:]
manifests = Path(workspace, "runs", "starlight").glob("*/run.json")
matches = [p for p in manifests if json.loads(p.read_bytes()).get("slurm_job_id") == jobs]
if len(matches) != 1:
    raise SystemExit(f"Expected one run manifest for Slurm job IDs {jobs}")
print(matches[0])
PYCODE
}
BUILD_RUN="$(find_build_run "$BUILD_JOBS")"
"$NSB_DATA" run status --run "$BUILD_RUN"
# After all 3,386 Gaia partitions complete:
"$NSB_DATA" dataset starlight validate --config "$CFG" --executor local

BUNDLE="$WORKSPACE/bv-candidate"
BASELINE="$REPO/crates/nsb/data/starlight_nside128.compact.release.bin"
"$NSB_DATA" dataset starlight diagnose compare-bv-integrated \
  --baseline "$BASELINE" \
  --baseline-sha256 11479bd0b6838d06df884b76dd74575bbc42d424ec85defecf847257c0f64d7f \
  --candidate "$BUNDLE/starlight.compact.bin" \
  --candidate-sha256 "$(sha "$BUNDLE/starlight.compact.bin")"
```

Verified `cache/objects/sha256` bytes and `cache/receipts` can be linked for
reuse when their inventory identities and recorded object paths remain
accessible from all workers. Source acquisition validates cached receipts.
Do not copy the previous run's worker metadata or integrated-only shards.
For the unchanged configuration, retry missing or failed workers with
`"$NSB_DATA" run resume --run "$BUILD_RUN"` after diagnosing the worker error
and waiting for the previous job to terminate. Resolve the returned Slurm
job IDs with `find_build_run` before checking status again. When the run has
no incomplete workers, resume reports that all partitions are complete. A changed input pin, software identity or scientific policy
needs a fresh generation workspace. See the [partition and cache
contract](map-generation.md).

Dataset validation reconciles fresh shards, independently checks deterministic
merge and population accounting, and writes the integrated CSV, merge report,
technical validation report and compact v2 bundle. Keep all four legacy fields
bitwise identical over 196,608 RING pixels. Compare admitted/excluded counts,
selection policy and calibration/inventory identities against the original
merge evidence as well as the approved v1 bytes. The supplement build and
artifact digests are expected to change. Any numeric or population drift blocks
this handoff gate pending investigation. Save the differential output with
the actual checksums, software identity and counts.

### 4. Score an independent full-sky reference

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
REGIONS="$REPO/docs/nsb_components/starlight/validation/regions-v1.json"
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

### 5. Review and recovery

- An unsupported inference cell identifies the Gaia source and the route,
  magnitude/colour bin and sky cell. Check the calibration report and
  input photometry. Improve representative support and rerun with a new
  checksum and workspace. Do not substitute masked labels or drop sources.
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
