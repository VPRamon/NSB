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
  completeness weights. B/V uses that weight once per admitted source and
  no weight for the separate supplement. B/V treats selection sigma as a
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

1. Build source reference rows with
   `nsb-data dataset starlight diagnose bv-xp --xp-continuous XP.csv
   --gaia-source GaiaSource.csv --output references.csv`. Supply matched
   Gaia DR3 DataLink or equivalent coefficient CSV rows and GaiaSource
   photometric columns. Pin the CSV contents and the XP design SHA-256.
2. Freeze sky/magnitude/colour bin edges and minimum training and validation
   support before evaluating test data. Run
   `python3 crates/nsb-data-tools/tools/calibrate_starlight_bv.py references.csv
   policy.json held-out.json --candidate-calibration bv.json`.
   The policy supplies `seed`, `sky_nside` (1, 2, 4 or 8), `g_edges`,
   `bp_g_edges`, `g_rp_edges`, `min_training`, `min_validation`,
   `min_validation_sky_cells`, `xp_design_sha256`,
   `xp_shared_calibration_fraction`, and `software_commit`. The code
   assigns whole Galactic NESTED nside-16 cells to train, validation and
   test using SHA-256. Use a separate mask stress test when appropriate.
   Its output is report-only. Archive the split, input, report and model
   digests with the calibration. The two XP calibration fractions require
   an external spectrophotometric justification.
3. Start a new generation workspace from the combined Ladon configuration
   and the approved run's supplement and policy pins. Rebuild the pinned
   bright-star artifact with `include_bv_photometry = true`. Keep the
   catalogue, passband, CK04, and existing integration identities fixed.
   Set `[starlight.bv_photometry]` to the candidate artifact path and SHA-256.
   Reuse verified Gaia objects and run fresh worker shards. Leave review-
   controlled inputs outside Git. See [generation](map-generation.md) and
   [reacquisition](licensing/REACQUISITION.md).
4. Run the existing merge/validate command. After a complete deterministic
   merge, `WORKSPACE/bv-candidate/` contains the compact map, a checksum
   manifest, and NESTED uncertainty evidence. The latter includes flux,
   the statistical and independent systematic covariance triplets,
   shared group sigmas, and source-route counts. It points back to the
   exact integrated map and merge report. The standard scientific gate
   reports B/V production admission as pending. The v1 production
   manifest cannot admit B/V bytes. The compact candidate can be loaded
   through `StarlightMap::from_compact_bytes` for Rust research queries.

Use the [approved artifact registration](release-candidate/runtime-assets-v1.toml)
as the full-run integrated baseline. Run
`nsb-data dataset starlight diagnose compare-bv-integrated --baseline v1.bin
--baseline-sha256 SHA1 --candidate v2.bin --candidate-sha256 SHA2`. It compares
the four integrated fields bitwise at every RING index. Compare population
counts and selection/calibration identities in the corresponding run evidence. Separate expected differences in
provenance (including the regenerated bright-star artifact checksum) from
scientific differences. Require a reviewed explanation for any numeric drift.

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
