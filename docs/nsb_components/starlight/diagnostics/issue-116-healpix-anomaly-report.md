# Issue #116 — HEALPix flux anomaly investigation report

Status: **corrected candidate regenerated on Ladon (3386 partitions); evidence published in PR #117.**

## Summary

The superseded UV-v2 candidate (`starlight_nside128.csv`, SHA-256
`5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`, frozen at
`docs/nsb_components/starlight/diagnostics/fixtures/starlight_nside128_legacy_frame_bug.csv`)
showed six large NSIDE=2-aligned regions with elevated `flux_ph_m2_s / admitted_sources`.
Automated diagnostics (`crates/nsb-data-tools/src/starlight/diagnostics.rs`)
reproduce the six anomalous parent cells **0, 16, 18, 26, 27, 43** on the legacy map.

## Confirmed bugs (fixed in PR #117)

| Bug | Fix |
|-----|-----|
| Gaia equatorial `source_id` HEALPix indices were accumulated into a map declared `coordinate_frame=galactic` without an ICRS→Galactic transform | Final accumulation uses GaiaSource `ra`/`dec` (ICRS) → Galactic nested pixel; selection lookup remains equatorial `source_id` HEALPix |
| Numeric `healpix.abs_diff` used as spatial nearest-neighbour for sparse selection tables | Angular separation on the sphere |
| Approximate `source_id` pixel centres used for canonical output | Production path requires parsed GaiaSource `ra`/`dec`; invalid rows are skipped at ingest |

## Full-sky before/after (corrected candidate)

Global median `flux_ph_m2_s / admitted_sources` ≈ **1.67×10⁶** on the corrected map.
Legacy six parents vs corrected median ratios (5× anomaly threshold):

| NSIDE=2 parent | Legacy (approx) | Corrected |
|---:|---:|---:|
| 0 | ~10.7× | **0.99×** |
| 16 | ~19× | **2.87×** |
| 18 | ~23× | **1.01×** |
| 26 | ~22× | **1.01×** |
| 27 | ~18× | **5.71×** (still elevated) |
| 43 | ~6× | **0.99×** |

Five of the six historical patches normalized. Parent **27** remains ~5.7× above the
global median and is flagged by the 5× diagnostic threshold; the new anomalous parent
set is `[9, 11, 13, 25, 27, 32, 33, 37]` rather than the legacy six-cell pattern.

**Conclusion:** The coordinate-frame bug is strongly implicated and explains the
large-amplitude legacy discontinuities; residual elevation at parent 27 is noted for
#103 scientific review but does not reproduce the original six-patch morphology.

## Eliminated hypotheses (quantitative)

| Hypothesis | Evidence |
|------------|----------|
| Selection weights at G=17 alone explain six patches | Pinned artifact `1a3670b5…` has ~0.994 completeness and ~1.005 weights at G=17 across all 48 NSIDE=2 parents; insufficient alone to explain >10× flux/source jumps |
| Sparse index-distance fallback as primary cause | Even before the angular fix, weight variation across parents was <0.4%; fixed regardless |

## Implementation

1. **Accumulation:** `galactic_nested_pixel_from_icrs_position(ra, dec, nside)` using Siderust `ICRS` → `Galactic`.
2. **Selection lookup:** `gaia_source_id_equatorial_nested_pixel(source_id, nside)` (separate contract).
3. **Diagnostics:** `analyse_candidate_path`, `analyse_workspace_shards`, example `issue116_analyze_shards`.
4. **Hydra config:** `crates/nsb-data-tools/config/starlight-production-300-650.hydra.toml`.

## Regeneration (Hydra / Ladon)

Workspace: `/mnt/beegfs/valles/nsb-data/starlight-production-300-650-fix116`
(reuses checksum-verified CAS cache + inventories from `starlight-production-300-650`).

- **3386/3386** partition shards completed (Slurm arrays `196957`–`196960`).
- `dataset starlight validate --executor local` → **passed**
- `dataset starlight publish --executor local` → **succeeded**

## Checksums

| Artifact | SHA-256 |
|----------|---------|
| Legacy candidate (superseded) | `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563` |
| Corrected candidate | `b17124d057faad2445575239c04928514d2846ec36a2f5df7137566058d85154` |
| Merge report | `52ca4a9d30c82f5d76532bbeccb9c829f6cf60ae1364ee9b9982683c54820c43` |
| Packed runtime map (production admission) | `a458debfd4665b590d27f952352a0d3f69b33d88635ed08c587202ff8a30bab3` |

## #103 status

Human scientific and redistribution decisions remain **pending** on the new candidate
identity. This report does not approve promotion.
