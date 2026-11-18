# Starlight B/V candidate for issue #317

The bundled Starlight product contains only the 300–650 nm radiance and its
three uncertainties. In v1, zero B/V fields indicate absent diagnostics.
Issue #317 tracks a source-derived, top-of-atmosphere extension. The current
work stages a candidate and leaves its scientific and redistribution admission
pending. [Issue #317](https://github.com/VPRamon/NSB/issues/317)

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

TBC

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

From the repository root, set `HANDOFF` to a new absolute directory on shared
storage. Place the input extracts and preregistered policy in that directory.
The binary must be available at the same path on the submission host and
workers.

```bash
cd /absolute/path/to/NSB
REPO="$PWD"
HANDOFF=/absolute/shared/path/starlight-bv-317
WORKSPACE="$HANDOFF/map"
mkdir -p "$HANDOFF"
CARGO_TARGET_DIR="$REPO/target" cargo build --locked -p nsb-data-tools --release
NSB_DATA="$REPO/target/release/nsb-data"
COMMIT="$(git rev-parse HEAD)"
sha() { sha256sum "$1" | cut -d ' ' -f 1; }

"$NSB_DATA" dataset starlight diagnose bv-xp \
  --xp-continuous "$HANDOFF/xp-continuous.csv" \
  --gaia-source "$HANDOFF/gaia-photometry.csv" \
  --output "$HANDOFF/xp-labels.csv"
python3 "$REPO/crates/nsb-data-tools/tools/calibrate_starlight_bv.py" \
  "$HANDOFF/labels.csv" "$HANDOFF/policy.json" "$HANDOFF/held-out.json" \
  --candidate-calibration "$HANDOFF/bv-calibration.json"
```

`bv-xp` joins plain Gaia DataLink or bulk-format continuous coefficient CSV
(including BP/RP parameter counts, deviations, coefficients, errors and full
correlation triangles) to one equally sized GaiaSource extract on unique
`source_id`. The extract contains `ra,dec,phot_g_mean_mag,phot_bp_mean_mag,
phot_rp_mean_mag` at Gaia DR3 astrometry. For training, `labels.csv` has
`source_id,galactic_nest16,g,bp,rp,b_w_m2_nm,v_w_m2_nm`. BP/RP blanks mean
genuinely unavailable photometry. Use `xp-labels.csv` for routes it actually
represents. Append naturally incomplete matched sources with wavelength-matched
external spectral labels if necessary, under the identical schema and flux
unit. Preserve IDs, NESTED Galactic coordinates, signed label fluxes, and the
optional XP covariance columns for audit. Document sample selection, coverage,
absolute calibration, label noise, variability, and upstream identities.
Each source appears once. When controls use artificial masking, run separately
with `--stress-test-mask-complete` and omit `--candidate-calibration`.

The policy has keys `seed` (u64), `sky_nside` (1, 2, 4 or 8), strictly
increasing `g_edges`, `bp_g_edges` and `g_rp_edges`, `min_training`,
`min_validation`, `min_validation_sky_cells`, `reference_provenance`
(nonempty description and auditable label origin), `xp_design_sha256`,
`xp_shared_calibration_fraction` (two positive fractions in B, V order
supported by independent absolute standards), and `software_commit` (full
Git SHA). Freeze the edges, sample sufficiency and acceptance criteria before
unblinding the test. Whole Galactic NESTED nside-16 cells go to deterministic
disjoint training, validation and test roles. Validation estimates the
correlated residual envelope and test scores never alter fitted moments.
The predictive scatter retains noise in the labels. Review coverage and any
covariance inflation against independent data, especially where XP is noisy.
Review attempted versus scored rows, out-of-domain rows, bias, absolute
relative-error tails, and 68%/95% coverage in the report’s route/region
summaries and its detailed magnitude/colour cells. The export guard requires training and validation support for every
modelled cell and held-out support for all four routes. Preset science
acceptance thresholds and adequate representative coverage of the admitted
population still require review. In particular, justify the conditional
population prior for G-only and verify faint and crowded populations against
the XP-selected distribution.

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
  "$HANDOFF/bv-calibration.json" "$(sha "$HANDOFF/bv-calibration.json")"

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

## Validation and open gates

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
  A full 300–650 nm production differential awaits the original pins
  and a valid B/V model.
- **Full-sky science.** Use the preregistered [NSB regions and
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
  workspace remain external. A representative non-XP model, CK04 B/V
  validation, full regeneration, independent full-sky validation, and a
  checksum-specific redistribution decision are pending. The checked-in
  v1 default remains the approved runtime asset. See the
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
