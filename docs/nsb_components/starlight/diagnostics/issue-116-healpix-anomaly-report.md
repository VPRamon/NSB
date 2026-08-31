# Issue #116 — HEALPix flux anomaly investigation report

Status: **root cause for remaining patches identified; hierarchical selection lookup fix implemented; smoke validation in progress on Ladon.**

## Executive summary

Two independent bugs produced HEALPix-aligned `flux_ph_m2_s / admitted_sources` discontinuities:

1. **ICRS→Galactic accumulation mismatch** (fixed in first PR #117 pass): Gaia equatorial `source_id` HEALPix indices were accumulated into a map declared `coordinate_frame=galactic`. This explains the legacy **six-parent** morphology (parents 0, 16, 18, 26, 27, 43).

2. **Sparse selection-table spatial resolution** (fix in progress): The pinned selection artifact (`1a3670b5…`) stores `completeness_table` entries at **NSIDE=128** but only **4096 unique healpix cells** — exactly **one tabulated representative per NSIDE=32 parent** for 4096 of 12288 parents. Production code resolved all other NSIDE=128 pixels via **angular nearest-neighbour** among the 4096 seeds, creating artificial Voronoi boundaries misaligned with HEALPix hierarchy. Because candidate flux is **selection-weighted** (`weight × flux` divided by unweighted admitted count), these boundaries appear as sharp patches in `flux / admitted_sources`.

The corrected candidate (`b17124d0…`) removed the legacy six-patch pattern but retained eight anomalous NSIDE=2 parents `[9, 11, 13, 25, 27, 32, 33, 37]` with parent 27 still ~5.7× the global median — consistent with bug (2) surviving the frame fix.

## Classification

### CONFIRMED BUGS

| Bug | Evidence | Fix |
|-----|----------|-----|
| Galactic map accumulated equatorial `source_id` HEALPix | Legacy six-parent pattern; frame test in `starlight_healpix_semantics.rs` | `galactic_nested_pixel_from_icrs_position(ra, dec)` |
| Selection lookup used `source_id` equatorial pixel instead of source ICRS position | Worker now uses `icrs_equatorial_nested_pixel(ra, dec)` at artifact `healpix_nside` | `worker.rs` `selection_weight()` |
| Sparse `completeness_table` resolved via angular nearest-neighbour at NSIDE=128 | 4096/196608 tabulated cells; production test shows >25% pixel disagreement vs hierarchical NSIDE=32 resolve; angular method creates more G=17 weight discontinuity edges | `build_hierarchical_resolve_map()` with NSIDE=32 parent inheritance + parent-level fallback for missing parents |
| Wrong logistic `m10_to_completeness` when `m10_map` is used | 25 golden cases vs GaiaUnlimited `surveyTCG.m10_to_completeness` | `cantat_gaudin_m10_to_completeness()` |
| XP path skipped `duplicated_source` / non-stellar exclusions | Code audit; regression test `xp_path_applies_same_scientific_exclusions_as_non_xp_path` | Shared `scientific_exclusion_reason()` on both paths |
| `faint_tail_flux_fraction` computed but never applied to flux | Code audit; merge report over-claimed residual correction | Reporting contract clarified in `map/product.rs` (systematic fraction only) |

### CONFIRMED ROOT CAUSE (legacy six patches)

**ICRS→Galactic accumulation mismatch** — quantitatively eliminates five of six legacy anomalous parents in the regenerated candidate.

### CONFIRMED ROOT CAUSE (remaining patches)

**Selection-weight spatial lookup on a NSIDE=32-subsampled completeness table using NSIDE=128 angular Voronoi tessellation**, combined with **weighted numerator / unweighted denominator** in the published map.

Causal chain (faint-G regime, where completeness spans 0.24–0.84 and weights reach 4.2×):

```
sparse completeness_table (4096 NSIDE=32 representatives)
  → angular nearest-neighbour at NSIDE=128 (old code)
  → discontinuous selection_weight(healpix, G, BP-RP) for G ≳ 19
  → weighted_flux / admitted_sources shows sharp boundaries
```

At G≈17 alone, weight variation is only ~1.002–1.035 (insufficient for >5× flux/source jumps). Remaining bright patches therefore also involve **spatial admission/exclusion structure** (`invalid_uv_predictors`, branch mix) under investigation.

### CONTRIBUTING FACTORS

- Selection artifact JSON omits explicit `coordinate_frame`, `ordering`, and `table_spatial_nside` (serde defaults to equatorial/nested; `table_spatial_nside` inferred at load).
- `invalid_uv_predictors` excludes ~263M sources (14.5% of observed), spatially heterogeneous — can modulate admitted population but does not explain NSIDE=128 Voronoi boundaries in weighted flux.
- Candidate stores weighted flux with unweighted admitted counts (diagnostic confound; not a bug per se).

### ELIMINATED HYPOTHESES

| Hypothesis | Evidence |
|------------|----------|
| Selection weights at G=17 alone explain six legacy patches | ~0.994 completeness at G=17 across parents; weight variation <0.4% before spatial lookup fix |
| UV correction primary cause of bright patches | Patches visible in 336–650 weighted flux; UV acts downstream |
| Wrong selection coordinate frame (equatorial vs galactic) for output | Output correctly Galactic; selection correctly separate equatorial contract |
| Partition/shard merge nondeterminism | Deterministic merge reproduced identical candidate SHA |

### KNOWN LIMITATIONS

- Full-sky regeneration with hierarchical selection fix pending smoke validation.
- `faint_tail_flux_fraction` not applied to flux (documented; separate contract issue).
- 8192 of 12288 NSIDE=32 parents have no direct table entry; hierarchical resolve uses parent-centre angular fallback to nearest tabulated parent.
- Human #103 decisions remain **pending**.

## H1 — Selection artifact provenance

Path: `/mnt/beegfs/valles/nsb-data/starlight-calibration/artifacts/selection-artifact.json`  
SHA-256: `1a3670b56eedaf9f9de0b32f081ccfa2baf741a449cd70c2be37d666101a9711`

| Field | Value |
|-------|-------|
| `schema_version` | 1 |
| `healpix_nside` | 128 |
| `coordinate_frame` | **absent** (defaults equatorial) |
| `ordering` | **absent** (defaults nested) |
| `table_spatial_nside` | **absent** (inferred: 32) |
| `m10_map` | empty |
| `completeness_table` | 212992 entries, **4096 unique healpix** |
| `training_command` | `python3 /home/valles/nsb-calibration/scripts/build_selection_artifact.py` |
| Reference | Cantat-Gaudin DR3, DOI 10.5281/zenodo.8063930 |
| Reference file | `allsky_M10_hpx7.hdf5` (SHA `43ca2c51…`) |

**Scientific-contract gap:** frame/order/table resolution are not serialized in the artifact; code must infer or fail closed.

## H2 — M10 → completeness

Old approximation `1/(1+exp(1.5*(G-M10)))` replaced with published Cantat-Gaudin / GaiaUnlimited `surveyTCG` piecewise sigmoid (10 hyperparameters). **25 golden cases** verified against `gaiaunlimited.selectionfunctions.surveyTCG.m10_to_completeness`. Production path uses `completeness_table`, not `m10_map`; formula fix guards future artifacts.

## H3/H4 — Selection weights vs flux patches

At G=17, BP−RP=0.8: angular NSIDE=128 resolve produces **more weight-discontinuity edges** than hierarchical NSIDE=32 resolve on the production artifact (see ignored test `production_artifact_angular_nearest_creates_sharper_weight_boundaries_than_hierarchical`).

Published map = `sum(weight_i × flux_i) / N_admitted`. Patches can appear even when raw stellar population is smooth.

## Boundary discontinuity metric

`boundary_discontinuity_report()` compares median |Δ log10(flux/admitted)| across NSIDE=2 parent boundaries vs within parents. Legacy candidate shows elevated cross/internal ratio; corrected frame-fix candidate still shows residual elevation.

## Checksums

| Artifact | SHA-256 |
|----------|---------|
| Legacy candidate (frame bug) | `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563` |
| Frame-fixed candidate (pre-selection fix) | `b17124d057faad2445575239c04928514d2846ec36a2f5df7137566058d85154` |
| Merge report (frame-fixed) | `52ca4a9d30c82f5d76532bbeccb9c829f6cf60ae1364ee9b9982683c54820c43` |

## Ladon smoke validation (48 partitions)

Workspace: `/mnt/beegfs/valles/nsb-data/starlight-smoke-fix116-selection` (48 partitions, shared CAS cache).

| Metric | Frame-fix only (old shards) | Hierarchical selection (new shards) |
|--------|----------------------------|-------------------------------------|
| Shard SHA (partition `000000-003111`) | `336fe2b0…` | `4329c62b…` (differs — selection path active) |
| Anomalous NSIDE=2 parents (5× threshold) | `[13, 24, 32, 33, 36, 37, 43]` | `[13, 24, 32, 33, 36, 37, 43]` |
| Boundary cross/internal ratio | 0.375 | 0.372 |

Shard-level flux **does change** with the hierarchical resolve, but this partial-sky subset does not yet show reduced parent anomalies. Full-sky regeneration is required before claiming patch elimination. Faint-G weight variation (up to 4.2× at G=20) is the regime where spatial resolve errors matter most.

## #103 status

Do **not** approve promotion until post-selection-fix candidate is validated.
