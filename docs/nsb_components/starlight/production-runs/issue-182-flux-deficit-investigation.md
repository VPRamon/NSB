# Starlight flux deficit vs nsb2 — issue #182 investigation

Status: **in progress**. This note records reproducible evidence gathered on
branch `fix/starlight-flux-deficit-182`. It is cross-implementation analysis,
not an independent astrophysical validation, and must not be promoted to
`scientifically_validated = true`.

Pinned #182 baseline (from `VPRamon/nsb-validation`):

| Quantity | Value |
|---|---:|
| NSB candidate commit | `53670d7b3be25cf99be7d13a9f2079d2145a0eb7` |
| NSB candidate SHA-256 | `76191c8b682d96adfc3a017f44f3fcfd0bec5dcb9a958d31668250b8a0ba396a` |
| nsb2 commit | `bc9320db03fbff69997ce366c6e76a37339c5d00` |
| Integrated relative bias | **−24.4%** |
| Correlation (all sky) | 0.223 |
| Correlation (excl. brightest 1%) | ~0.94 |

Candidate band diagnostics (frozen merge report):

| Band | Integrated flux (ph m⁻² s⁻¹) | Fraction of combined |
|---|---:|---:|
| 300–336 nm (UV corrected) | 2.239×10¹⁴ | 2.80% |
| 336–650 nm (measured) | 7.785×10¹⁵ | 97.20% |
| 300–650 nm (combined) | 8.009×10¹⁵ | 100% |

## Hypothesis status

| ID | Hypothesis | Status | Contribution to −24.4% |
|---|---|---|---|
| H2 | Whole-source exclusion on `invalid_uv_predictors` discards large measured flux | **Partially explanatory as a mechanism; radiometrically minor on pilot sample** | Pilot 2 partitions: **~0.08% of estimated nsb2 total** (see Experiment C) |
| H1 | Missing XHIP/Hipparcos bright-star supplement | **Partially explanatory** | **~7.4% of nsb2 flux**; removes ~6 pp of bias (−24.4%→−18.3%); correlation 0.22→0.87 |
| Model / population residuals | XP vs Pickles, selection weighting, Gaia bright-catalogue treatment, faint population | **Dominant residual after removing XHIP** | Remaining integrated bias **−18.3%** vs nsb2 without XHIP |

## Experiment C — flux by exclusion reason (pilot)

**Hypothesis under test:** if `invalid_uv_predictors` exclusions carry a large
fraction of selection-weighted 336–650 nm flux, H2 can explain a material share
of the −24.4% deficit.

**Distinguishing result:** lost flux ≪ 24% ⇒ H2 is not the dominant radiometric
cause; lost flux ~24% ⇒ H2 is dominant.

### Pilot run (2 smoke partitions)

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
| Lost / estimated nsb2 total (using 0.756 ratio) | **0.083%** |

Interpretation: on this sample, UV-predictor exclusions are numerous in the
global catalogue but **faint**. They cannot explain the −24.4% integrated
deficit. A 48-partition smoke and/or full-sky attribution remains required to
bound sample variance; early evidence strongly disfavours H2 as the primary
cause.

Global catalogue context (for scale only): merge report records
`invalid_uv_predictors = 263,033,147` (~14.5% of observed sources). Even if the
flux fraction scaled with the higher source fraction relative to this pilot,
the implied deficit would remain far below 24% unless those partitions are
dramatically brighter than the pilot (not suggested by the faint mean).

## Policy change (scientifically justified; not a deficit tuner)

Even though H2 is radiometrically minor, whole-source exclusion of sources with
valid measured 336–650 nm flux solely because `bp_rp` is missing is an
**unjustified admission policy**.

Implementation on this branch:

- When UV predictors are unavailable, production now **retains**
  selection-weighted measured 336–650 nm flux.
- UV component is marked `ApplicabilityStatus::Unavailable` /
  `EvaluationDecision::MeasuredOnly` (0.0 recorded as *not estimated*, not as a
  calibrated UV prediction).
- A 5% measured-flux systematic covers missing UV without inventing a point
  estimate (~global UV/measured ≈ 2.8% for in-domain admissions).
- Policy IDs bumped: `gaia-dr3-full-population-v2`,
  `gaia-xp-continuous-uv-corrected-300-650-v2`.
- Flux-attribution diagnostics intentionally still apply the **legacy**
  exclusion so Experiment C can quantify the historical loss.

The frozen #182 candidate map is **unchanged** until a deliberate production
rebuild. Do not claim the published candidate already includes this fix.

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

Local artifacts (not committed): `nsb-validation/work/expB/`,
`nsb-validation/results/expB-{full,no-xhip}/`.

## Experiment A / B / D tooling

Independent validation harness changes live in `VPRamon/nsb-validation` branch
`feat/starlight-flux-deficit-182-experiments`:

- no-XHIP + per-component nsb2 reference generation
- configurable 336–650 nm bandpass
- brightness-tail / missing-flux metrics in compare reports

A `workflow_dispatch` of `Full Starlight cross-validation` was started on that
branch to regenerate the full reference and compare artifacts.

## Next measurements (priority)

1. Finish smoke-48 / full-sky Experiment C to close H2 radiometrically at catalogue scale.
2. Finish component-separated nsb2 maps (Gaia bright / faint / XHIP) and
   brightness-tail missing-flux allocation (Experiments B+D).
3. Export NSB 336–650 nm from existing shards and compare to nsb2 336–650
   (Experiment A) to separate UV policy from measured-band model differences.
4. Full Gaia DR3 rebuild is **not** justified solely by the UV-retention policy
   fix (pilot radiometric impact ≪ 1%). Rebuild only if a larger measured-band
   or bright-catalogue defect is confirmed.

## Classification glossary (for the eventual #182 close-out)

| Class | Meaning |
|---|---|
| Confirmed defect | Implementation bug or unjustified policy with measurable wrongness |
| Scientific-policy issue | Behaviour that is deliberate but needs an explicit versioned choice |
| Expected model difference | XP vs Pickles / UV / selection / bright-star treatment residuals |
| Unresolved discrepancy | Not yet attributed after available experiments |
