# Issue #116 — HEALPix flux anomaly investigation report

Status: **generator fixes landed; full corrected candidate regeneration in progress on Hydra (Ladon).**

## Summary

The frozen UV-v2 candidate (`starlight_nside128.csv`, SHA-256
`5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`) shows six
large NSIDE=2-aligned regions with elevated `flux_ph_m2_s / admitted_sources`.
Automated diagnostics (`crates/nsb-data-tools/src/starlight/diagnostics.rs`)
reproduce the six anomalous parent cells **0, 16, 18, 26, 27, 43** on the legacy
map.

## Confirmed bugs (fixed in PR #117)

| Bug | Fix |
|-----|-----|
| Gaia equatorial `source_id` HEALPix indices were accumulated into a map declared `coordinate_frame=galactic` without an ICRS→Galactic transform | Final accumulation uses GaiaSource `ra`/`dec` (ICRS) → Galactic nested pixel; selection lookup remains equatorial `source_id` HEALPix |
| Numeric `healpix.abs_diff` used as spatial nearest-neighbour for sparse selection tables | Angular separation on the sphere |
| Approximate `source_id` pixel centres used for canonical output | Production path requires parsed GaiaSource `ra`/`dec`; invalid rows are skipped at ingest |

## Causal evidence (in progress)

**The coordinate-frame bug is confirmed, but it is not yet proven to be the sole
cause of the six-patch amplitude discontinuity.**

Controlled 48-partition build on Hydra (`starlight-production-300-650-fix116`,
corrected generator, pinned artifacts) shows:

| NSIDE=2 parent | Legacy candidate median ratio | Corrected subset median ratio |
|---:|---:|---:|
| 0 | ~10.7× global | ~1.03× global |
| 16 | ~1.9× | ~2.58× |
| 18 | ~2.3× | ~0.96× |
| 26 | ~2.2× | (no subset coverage) |
| 27 | ~1.8× | (no subset coverage) |
| 43 | ~6.2× | ~10.9× (subset; full-sky pending) |

Full 3386-partition Slurm regeneration is required before closing the causal
question and #116.

## Eliminated hypotheses (quantitative, not complete)

| Hypothesis | Evidence |
|------------|----------|
| Selection weights at G=17 alone explain six patches | Pinned artifact `1a3670b5…` has ~0.994 completeness and ~1.005 weights at G=17 across all 48 NSIDE=2 parents; insufficient alone to explain >10× flux/source jumps |
| Sparse index-distance fallback as primary cause | Even before the angular fix, weight variation across parents was <0.4%; fixed regardless |

**Not eliminated:** selection-function effects across the full magnitude/colour
population; stage-local multiplicative terms (UV, photometric inference); shard
merge semantics. Stage diagnostics and full-sky before/after comparison are
pending full regeneration.

## Implementation

1. **Accumulation:** `galactic_nested_pixel_from_icrs_position(ra, dec, nside)` using Siderust `ICRS` → `Galactic`.
2. **Selection lookup:** `gaia_source_id_equatorial_nested_pixel(source_id, nside)` (separate contract).
3. **Diagnostics:** `analyse_candidate_path`, `analyse_workspace_shards`, example `issue116_analyze_shards`.
4. **Hydra config:** `crates/nsb-data-tools/config/starlight-production-300-650.hydra.toml`.

## Regeneration (Hydra / Ladon)

Workspace: `/mnt/beegfs/valles/nsb-data/starlight-production-300-650-fix116`
(reuses checksum-verified CAS cache + inventories from `starlight-production-300-650`).

```bash
cargo run --release -p nsb-data-tools --bin nsb-data -- \
  dataset starlight build \
  --config crates/nsb-data-tools/config/starlight-production-300-650.hydra.toml \
  --executor slurm
```

## Checksums

| Artifact | SHA-256 |
|----------|---------|
| Legacy candidate (superseded) | `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563` |
| Corrected candidate | **pending full Hydra build** |

## #103 status

Human scientific and redistribution review remains **PENDING**. Update review
decision files only after the corrected candidate SHA-256 is frozen.
