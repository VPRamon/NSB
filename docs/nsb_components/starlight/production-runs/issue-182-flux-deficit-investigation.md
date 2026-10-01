# Starlight flux deficit vs nsb2 — issue #182 investigation

Status: **in progress**. This note records reproducible evidence gathered on
branch `fix/starlight-flux-deficit-182`. It is cross-implementation analysis,
not an independent astrophysical validation, and must not be promoted to
`scientifically_validated = true`. Do **not** claim #182 is solved while a true
`Measured336To650` production comparison remains outstanding.

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
| H2 | Whole-source exclusion on `invalid_uv_predictors` discards large measured flux | **Partially explanatory as a mechanism; radiometrically minor** | Best estimate (smoke-48): **~0.491% of estimated nsb2 total**; pilot-2 was **~0.083%** (see Experiment C) |
| H1 | Missing XHIP/Hipparcos bright-star supplement | **Partially explanatory** | **~7.4% of nsb2 flux**; removes ~6 pp of bias (−24.4%→−18.3%); correlation 0.22→0.87 |
| Model / population residuals | XP vs Pickles, selection weighting, Gaia bright-catalogue treatment, faint population | **Dominant residual after removing XHIP** | Remaining integrated bias **−18.3%** vs nsb2 without XHIP |

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

## Experiment A — combined-candidate measured-subcomponent diagnostic

**Reclassification:** the shard export used for Experiment A is a
**`combined-candidate-measured-subcomponent` diagnostic**, not a true NSB
`Measured336To650` production candidate. Source admission still inherits the
combined-product UV-predictor gate (`invalid_uv_predictors` sources never
entered those shards). Do **not** call this export a “true 336–650 NSB
artifact.”

Exported measured-band column from frozen combined-product shards
(`export-measured336650`):

| Field | Value |
|---|---|
| Class | `combined-candidate-measured-subcomponent` (provisional) |
| SHA-256 | `878a2e7cc3be1e83e5648194edde107bb50cb09fa484a21a1a95d6f49678a988` |
| Total measured flux | 8.682×10¹² ph m⁻² s⁻¹ |
| Parent combined SHA-256 | `76191c8b…` (distinct) |
| Manifest | `issue-182-measured-336-650-export-manifest.json` |

Comparison vs nsb2 336–650 (pinned `bc9320db`) — **provisional** under the
combined-candidate admission population:

| Reference | Bias | Corr | Corr excl. top 1% | Top 0.1% flux ratio |
|---|---:|---:|---:|---:|
| nsb2 **full** 336–650 | **−22.87%** | 0.228 | 0.942 | 0.128 |
| nsb2 **without XHIP** 336–650 | **−17.03%** | 0.868 | 0.942 | 0.501 |

Combined 300–650 (Experiment B) was −24.41% / −18.34%. Under this diagnostic
pin the measured-only column is only ~1.5 pp less deficit than combined, which
*suggests* the radiometric gap is primarily measured-band / Gaia-population
rather than the UV 300–336 correction — but that UV/admission split must be
**re-interpreted** once a true `Measured336To650` production candidate is
pinned.

A true `Measured336To650` production rebuild is in progress on Ladon
(workspace `starlight-production-336-650-issue182`; config
`crates/nsb-data-tools/config/starlight-production-336-650-issue182.ladon.toml`).
Until that lands and is compared, #182 remains open.

## Experiment A / B / D tooling

Independent validation harness changes live in `VPRamon/nsb-validation` branch
`feat/starlight-flux-deficit-182-experiments`:

- no-XHIP + per-component nsb2 reference generation
- configurable 336–650 nm bandpass
- brightness-tail / missing-flux metrics in compare reports
- component-identity and quantity-from-source-metadata contracts
- undefined relative metrics serialize as JSON `null` (not NaN)

NSB side: `nsb-data dataset starlight diagnose export-measured336650` derives a
measured-band **subcomponent** map from frozen combined-product shards
(distinct checksum + `physical_quantity=photon_radiance_336_650_nm` metadata)
so Experiment A cannot relabel a 300–650 candidate by caller string. That
export is **not** a substitute for a dedicated `Measured336To650` production
build.

## Next measurements (priority)

1. Finish / pin the true `Measured336To650` production candidate on Ladon and
   rerun Experiment A against it (replace/reclassify the provisional
   subcomponent pin).
2. Attribute the ~17–18% Gaia-only measured-band residual (bright catalogue vs
   faint map, XP vs Pickles, selection weighting, admission) without tuning to
   nsb2.
3. Bright-star supplement policy assessment (need ≠ copy XHIP).
4. Optional: full-sky Experiment C attribution to tighten the ~0.491% smoke-48
   estimate (not expected to change the “H2 is minor” conclusion).

## Classification glossary (for the eventual #182 close-out)

| Class | Meaning |
|---|---|
| Confirmed defect | Implementation bug or unjustified policy with measurable wrongness |
| Scientific-policy issue | Behaviour that is deliberate but needs an explicit versioned choice |
| Expected model difference | XP vs Pickles / UV / selection / bright-star treatment residuals |
| Unresolved discrepancy | Not yet attributed after available experiments |
