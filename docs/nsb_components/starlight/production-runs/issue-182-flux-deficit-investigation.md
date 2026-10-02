# Starlight flux deficit vs nsb2 — issue #182 investigation

Status: **in progress**. This note records reproducible evidence gathered on
branch `fix/starlight-flux-deficit-182`. It is cross-implementation analysis,
not an independent astrophysical validation, and must not be promoted to
`scientifically_validated = true`. A true `Measured336To650` production
comparison (Experiment A) is **completed**; #182 remains open because the
~16% Gaia-only residual and the missing very-bright population are not yet
resolved by a production-approved supplement.

Pinned #182 baseline (from `VPRamon/nsb-validation`):

| Quantity | Value |
|---|---:|
| NSB candidate commit | `53670d7b3be25cf99be7d13a9f2079d2145a0eb7` |
| NSB candidate SHA-256 | `76191c8b682d96adfc3a017f44f3fcfd0bec5dcb9a958d31668250b8a0ba396a` |
| nsb2 commit | `bc9320db03fbff69997ce366c6e76a37339c5d00` |
| Integrated relative bias | **−24.4%** |
| Correlation (all sky) | 0.223 |
| Correlation (excl. brightest 1%) | ~0.94 |

## Candidate band diagnostics (frozen merge report)

Exact frozen `#182` candidate `band_diagnostics` / `canonical_map` totals
(`crates/nsb/data/merge_report.json`):

| Band | Integrated flux (ph m⁻² s⁻¹) | Fraction of combined |
|---|---:|---:|
| 300–336 nm (UV corrected) | **2.81220866672×10¹¹** | ~3.14% |
| 336–650 nm (measured) | **8.68155054926×10¹²** | ~96.86% |
| 300–650 nm (combined) | **8.96277141593×10¹²** | 100% |

### Superseded incorrect denominator (do not reuse)

An early diagnostic table mis-read merge-report band totals at a ~10³-inflated
scale. Those figures are retained only as a historical warning:

| Band | Superseded incorrect denominator | Correct frozen value |
|---|---:|---:|
| 300–336 nm | 2.239×10¹⁴ | 2.812×10¹¹ |
| 336–650 nm | 7.785×10¹⁵ | 8.682×10¹² |
| 300–650 nm | 8.009×10¹⁵ | 8.963×10¹² |

**What was measured:** literal digits resembling merge-report totals, treated as
ph m⁻² s⁻¹ at face value for fraction estimates (including an early XHIP-only
bound).

**What was wrong:** the denominator was ~893× too large relative to the
validation loader / map-integral convention used by `nsb-validation`
(`sum(flux_ph_m2_s)` ≈ 8.963×10¹²).

**Why:** bookkeeping / unit-scale inconsistency when copying band diagnostics
into the early bound, not a change in the frozen candidate itself.

**Corrected value:** use the frozen merge-report table above (and the same
~10¹² scale in Experiment C / XHIP reconciliation).

## Hypothesis status

| ID | Hypothesis | Status | Contribution to −24.4% |
|---|---|---|---|
| H2 | Whole-source exclusion on `invalid_uv_predictors` discards large measured flux | **Partially explanatory as a mechanism; radiometrically minor** | True Measured336To650 ΔF ≈ **0.54%** of est. nsb2; smoke-48 ≈ **0.491%**; pilot-2 was **~0.083%** |
| H1 | Missing XHIP/Hipparcos bright-star supplement | **Partially explanatory** | **~7.4% of nsb2 flux**; removes ~6 pp of bias (−24.4%→−18.3%); correlation 0.22→0.87 |
| Model / population residuals | XP vs Pickles, selection weighting, Gaia bright-catalogue treatment, faint population | **Dominant residual after removing XHIP** | True Exp A no-XHIP residual **−16.4%**; vs Gaia-bright alone **−4.4%** |

## Experiment C — flux by exclusion reason

**Hypothesis under test:** if `invalid_uv_predictors` exclusions carry a large
fraction of selection-weighted 336–650 nm flux, H2 can explain a material share
of the −24.4% deficit.

**Distinguishing result:** lost flux ≪ 24% ⇒ H2 is not the dominant radiometric
cause; lost flux ~24% ⇒ H2 is dominant.

### Best estimate — smoke-48 (routing-fixed)

- Commit: `b96038e` diagnostic binary; report SHA-256
  `04ef4be9eff905584a3d6180b2ca59d0cdaf76c28d075ba7baaecc09499c8675`
- Artifact: `issue-182-experiment-c-smoke48.json` (Ladon `.../smoke48-fixed/`)
- Partitions: **48**

| Metric | Value |
|---|---:|
| Observed sources | **25,624,960** |
| Admitted | 21,581,555 |
| Excluded | 4,043,405 |
| `invalid_uv_predictors` | **3,565,954** (100% `missing_bp_rp`) |
| Lost weighted 336–650 / admitted combined | **≈ 0.650%** |
| Lost / estimated nsb2 total (0.756 ratio) | **≈ 0.491%** |

**Sampling limitation:** smoke-48 is a multi-partition sample, not a full-sky
attribution. Treat **~0.491%** as the current best *estimate* of the H2
reference contribution; do **not** claim an exact full-sky value from this
run. Full-sky attribution remains useful to tighten the bound.

**Scientific conclusion:** H2 remains radiometrically minor versus the
~24% discrepancy (≪ 24%), whether judged at pilot-2 or smoke-48 scale.

### Pilot-2 (superseded as headline; retained for history)

- Host: Ladon login node
- Workspace: `/mnt/beegfs/valles/nsb-data/starlight-production-300-650-fix116-photometric`
  (candidate SHA-256 `76191c8b…`, matching #182)
- Code commit: `3236950619deb906715716f5356b95f2ec54cede`
- Command:

```bash
nsb-data dataset starlight diagnose flux-attribution \
  --config $WS/config.toml --workspace $WS --repo-root $REPO \
  --commit $COMMIT --partitions pilot2/partitions.txt \
  --output pilot2/flux-attribution-pilot2.json
```

- Artifact: `/mnt/beegfs/valles/nsb-data/starlight-diagnostics-182/pilot2/flux-attribution-pilot2.json`

| Metric | Value |
|---|---:|
| Observed sources | 1,055,362 |
| Admitted | 1,000,593 |
| `invalid_uv_predictors` count | 17,169 (1.63% of observed) |
| All predictor failures | **100% `missing_bp_rp`** |
| Mean admitted combined flux / source | 2.39×10⁴ ph m⁻² s⁻¹ |
| Mean invalid_uv weighted 336–650 / source | 1.53×10³ ph m⁻² s⁻¹ (~15× fainter) |
| Lost weighted 336–650 / admitted combined | **0.110%** |
| Lost / estimated nsb2 total (using 0.756 ratio) | **~0.083%** (pilot-2 only) |

Pilot-2 first showed that UV-predictor exclusions are numerous but **faint**.
Smoke-48 raised the estimated lost / nsb2 fraction from **~0.083%** to
**~0.491%** without changing the scientific conclusion (still ≪ 24%).

Global catalogue context (for scale only): merge report records
`invalid_uv_predictors = 263,033,147` (~14.5% of observed sources). Even if the
flux fraction scaled with the higher source fraction relative to these samples,
the implied deficit would remain far below 24% unless those partitions are
dramatically brighter than the sampled ones (not suggested by the faint means).

## Policy change — superseded proposal vs retained honest contract

### Intermediate proposal (superseded; do not treat as production policy)

An earlier commit on this branch (`35bce6b`) proposed retaining measured
336–650 nm flux when UV predictors are unavailable, labelling the UV component
`ApplicabilityStatus::Unavailable` / `EvaluationDecision::MeasuredOnly`, adding
an arbitrary **5%** missing-UV systematic, and bumping policy IDs to `…-v2`.

That proposal is **withdrawn** for scientific-contract reasons:

- The canonical product is labelled **300–650 combined**. Publishing a
  measured-only lower bound under that label misrepresents spectral coverage.
- A global ~3% UV/measured ratio does **not** justify a 5% Gaussian-like
  systematic for the `bp_rp`-missing population, nor the
  `FullyCorrelatedBetweenSources` correlation inheritance that would add
  linearly across huge source counts.
- Investigation tooling must not be shipped as an unvalidated production policy
  merely because it moves NSB toward nsb2.

### Retained production contract (Option A)

- Sources lacking UV predictors remain **excluded** from the canonical
  combined 300–650 map (`invalid_uv_predictors`). **Production UV policy is
  unchanged.**
- Their measured 336–650 contribution is quantified by Experiment C
  diagnostics / sidecars, not admitted as incomplete full-band flux.
- Admission / spectral policy IDs remain **`gaia-dr3-full-population-v1`** /
  **`gaia-xp-continuous-uv-corrected-300-650-v1`**.
- A versioned policy registry validates historical merge reports by declared
  policy ID rather than requiring equality with whatever is currently emitted.

The frozen #182 candidate map is unchanged. Do not claim the published
candidate includes a UV-missing retention fix.

## Experiment B — XHIP / no-XHIP (completed locally)

**Hypothesis under test:** the 88-star XHIP supplement explains a material
fraction of the −24.4% integrated deficit and/or the top-0.1% residuals.

### Component-separated nsb2 reference (pinned `bc9320db`)

Generated with `nsb-validation` `nsb2-generate --write-components`
(~10 min). Reference SHA-256 (full):
`f21518839a03eea89ed9ee6fca4cd34600eac11fa45b154de045087c9e2cd69e`.

| Component | Sources | Fraction of nsb2 full integrated flux |
|---|---:|---:|
| `gaia_dr3_bright` | 36,908,056 | **81.3%** |
| `gaia_dr3_faint_map` | 12,582,912 | **11.3%** |
| `xhip_gaia_supplement` | 88 | **7.4%** |
| full (all three) | — | 100% |
| without XHIP | — | 92.6% |

### NSB candidate vs nsb2 full / no-XHIP

Reproduces the #182 baseline against the regenerated full map, then isolates XHIP:

| Reference | Integrated relative bias | Correlation | Top 0.1% flux ratio | Top 0.1% share of missing flux |
|---|---:|---:|---:|---:|
| nsb2 **full** | **−24.41%** | **0.223** | 0.125 | **37.8%** |
| nsb2 **without XHIP** | **−18.34%** | **0.865** | 0.478 | 12.5% |

Median / p68 / p90 / p95 relative errors are essentially unchanged by removing
XHIP (~5.5% / 12.9% / 36% / 50%), confirming XHIP is a **bright-tail /
correlation** effect more than a typical-pixel effect.

**Conclusion:** H1 is **partially explanatory**. XHIP accounts for roughly
one-quarter of the integrated deficit magnitude (about 6 percentage points)
and almost all of the catastrophic correlation collapse. A **−18.3%**
smooth deficit remains against Gaia-only nsb2 and must be attributed
elsewhere (measured-band model / population differences). An NSB XHIP-like
bright-star supplement is scientifically motivated for the extreme tail, but
must still clear provenance/licensing review before adoption.

### Reconciling the earlier ~0.008% “XHIP-only” bound (superseded)

An early diagnostic compared an XHIP-only radiance integral against the
candidate merge-report `band_diagnostics` totals that were mis-read at
~10¹⁵ ph m⁻² s⁻¹ scale (**superseded incorrect denominator**), yielding an
apparent XHIP fraction ~0.008%.

That bound is **superseded and wrong as a fraction of the validation total**:

- The frozen merge report on the #182 workspace records combined flux
  ≈ **8.96×10¹²** ph m⁻² s⁻¹ (same scale as the nsb-validation loader /
  `M2_S_TO_CM2_NS_RATE_FACTOR` path), not the superseded ~8×10¹⁵ figure.
- Component-separated nsb2 generation integrates each component with the same
  radiance × pixel-solid-angle convention used for the full reference, giving
  XHIP ≈ **7.4%** of nsb2 full.
- The cheap bound therefore mixed an inconsistent denominator (and possibly
  point-source vs map-integrated semantics) with the validator’s quantity.
  Prefer the component-map conservation check once flux-conservation tests land
  in `nsb-validation`.

Local artifacts (not committed): `nsb-validation/work/expB/`,
`nsb-validation/results/expB-{full,no-xhip}/`.

## Experiment A — true Measured336To650 production candidate

### Candidate lineage (do not conflate)

| Stage | Class | Status |
|---|---|---|
| Frozen combined `#182` map | `Combined300To650` (`76191c8b…`) | **Baseline** for original −24.4% |
| Shard `flux_336_650` export | `combined-candidate-measured-subcomponent` | **Diagnostic / provisional** (inherits UV gate) |
| Ladon Measured336To650 rebuild @ `3236950` | true `Measured336To650` | **Radiometric map** SHA `32d4f6d2…`; UV systematic bucket dual-filed (superseded bookkeeping) |
| Re-finalize @ HEAD (`…-refinalize` workspace) | true `Measured336To650` | **In progress** — expect identical selected flux map; `systematic_uncertainty_300_336 = 0` |

**True production candidate** (initial Ladon workspace
`starlight-production-336-650-issue182`, generating NSB commit `3236950`,
config SHA-256 `1b3c3649…`):

| Field | Value |
|---|---|
| Class | **true `Measured336To650` production candidate** |
| Canonical SHA-256 | `32d4f6d2557ead304ea07c09372c63b084d8f0d14ad751984d48915e7f8b9d47` |
| Total measured flux | **8.745736972625×10¹²** ph m⁻² s⁻¹ |
| Admitted / excluded | **1,781,834,843** / 29,874,928 |
| UV applied | **false** (`gaia-xp-continuous-336-650-v1`) |
| `invalid_uv_predictors` exclusions | **0** (reason absent) |
| Provenance | `issue-182-true-measured-336-650-provenance.json` |
| Bookkeeping caveat | `total_flux_300_336 = 0` but `systematic_uncertainty_300_336 ≠ 0` on this generating commit (dual-filing). Corrected in `236091c`; re-finalize must verify flux-map equality and zero UV systematic buckets. |

### Population / flux difference vs combined-candidate subcomponent

| Quantity | Combined-subcomponent (old Exp A) | True Measured336To650 | Δ |
|---|---:|---:|---:|
| Admitted sources | 1,518,801,696 | 1,781,834,843 | **+263,033,147** |
| Measured 336–650 flux | 8.68155054926×10¹² | 8.745736972625×10¹² | **+6.419×10¹⁰ (+0.74%)** |
| ΔF / est. nsb2 total (~1.186×10¹³) | — | — | **≈ 0.54%** |

This empirical UV-gate ΔF is **broadly consistent** with Experiment C smoke-48
(~0.491% of estimated nsb2 total); both remain radiometrically minor vs ~24%.

### Corrected Experiment A metrics vs nsb2 336–650 (`bc9320db`)

| Reference | Bias | Corr | Corr excl. top 1% | Top 0.1% flux ratio | Median abs rel |
|---|---:|---:|---:|---:|---:|
| nsb2 **full** | **−22.30%** | 0.228 | 0.941 | 0.128 | 0.0495 |
| nsb2 **without XHIP** | **−16.42%** | 0.867 | 0.941 | 0.502 | 0.0494 |
| nsb2 **Gaia bright only** | **−4.35%** | 0.843 | — | — | — |

p68 / p90 / p95 (full): 0.124 / 0.352 / 0.491. Brightness-bin missing-flux
share (full): 0–50% −8.9%; 50–90% −14.3%; 90–99% −18.2%; 99–99.9% −18.5%;
top 0.1% −87.2%.

### Superseded provisional subcomponent export (diagnostic only)

The earlier shard export remains available as a **diagnostic cross-check** only:

| Field | Value |
|---|---|
| Class | `combined-candidate-measured-subcomponent` |
| SHA-256 | `878a2e7cc3be1e83e5648194edde107bb50cb09fa484a21a1a95d6f49678a988` |
| Provisional full / no-XHIP bias | −22.87% / −17.03% |
| Manifest | `issue-182-measured-336-650-export-manifest.json` |

Do **not** cite those provisional biases as the measured-band product result.

## Experiment A / B / D tooling

Independent validation harness changes live in `VPRamon/nsb-validation` branch
`feat/starlight-flux-deficit-182-experiments`:

- no-XHIP + per-component nsb2 reference generation
- configurable 336–650 nm bandpass
- brightness-tail / missing-flux metrics in compare reports
- component-identity and quantity-from-source-metadata contracts
- undefined relative metrics serialize as JSON `null` (not NaN)

NSB side: `export-measured336650` remains a fail-closed **combined-candidate
measured-subcomponent** diagnostic. Experiment A pins now use the true
`Measured336To650` production candidate above.

## Updated #182 accounting (after true Exp A)

| Term | Contribution |
|---|---:|
| Original full-band discrepancy (NSB vs nsb2 full, 300–650) | **−24.4%** |
| XHIP / bright-star supplement | **~−6.1 pp** (Exp B: −24.4% → −18.3%); XHIP ≈ **7.4%** of nsb2 |
| UV/admission (true Measured336To650 vs combined population) | **≈ 0.5–0.7 pp** of integrated flux (~0.54% of est. nsb2; smoke-48 ≈ 0.49%) |
| Measured-band Gaia-only residual (true Exp A no-XHIP) | **≈ −16.4%** |
| vs nsb2 Gaia-bright alone | **≈ −4.4%** |
| Still unresolved (XP vs Pickles, selection, faint population, bright-catalogue treatment beyond XHIP) | Dominant residual after XHIP + UV gate |

`scientifically_validated` remains **false**. Issue #182 remains **open**.

## Next measurements (priority)

1. Re-finalize true Measured336To650 with corrected UV systematic bookkeeping;
   confirm canonical flux SHA equality vs `32d4f6d2…`.
2. Implement NSB-native very-bright supplement per
   [`../bright-stars/design-v1.md`](../bright-stars/design-v1.md) (external
   opt-in; do not copy XHIP; do not embed NC catalogue bytes).
3. Attribute the ~16% Gaia-only measured-band residual (selection weighting,
   XP vs photometric route, faint population) without tuning to nsb2.
4. Optional: full-sky Experiment C attribution to tighten the ~0.491% smoke-48
   estimate (not expected to change the “H2 is minor” conclusion).

## Classification glossary (for the eventual #182 close-out)

| Class | Meaning |
|---|---|
| Confirmed defect | Implementation bug or unjustified policy with measurable wrongness |
| Scientific-policy issue | Behaviour that is deliberate but needs an explicit versioned choice |
| Expected model difference | XP vs Pickles / UV / selection / bright-star treatment residuals |
| Unresolved discrepancy | Not yet attributed after available experiments |
