# Issue #116 — HEALPix flux anomaly investigation report

Status: root cause confirmed in generator; candidate regeneration pending verified Gaia inputs.

## Summary

The frozen UV-v2 candidate (`starlight_nside128.csv`, SHA-256
`5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`) shows six
large NSIDE=2-aligned regions with elevated `flux_ph_m2_s / admitted_sources`.
Automated diagnostics (`crates/nsb-data-tools/src/starlight/diagnostics.rs`)
reproduce the six anomalous parent cells **0, 16, 18, 26, 27, 43** on the legacy
map.

## Root cause (confirmed)

**Gaia equatorial HEALPix indices from `source_id` were accumulated directly
into a map declared `coordinate_frame=galactic` without an ICRS→Galactic sky
transform.**

The generator used bit-shifted `source_id` pixels for accumulation while the
runtime contract and packer treat nested indices as Galactic. Selection-function
lookup correctly uses equatorial pixels, but that path is **not** the cause of
the six patches (see eliminated hypotheses below).

## Eliminated hypotheses

| Hypothesis | Evidence against |
|------------|------------------|
| Selection-function completeness drives six NSIDE=2 patches | Pinned production artifact (`1a3670b5…`) has uniform completeness (~0.994) and weights (~1.005) across all 48 NSIDE=2 parents at G=17; no cell reaches the 5× weight cap. |
| Sparse selection nearest-neighbour by numeric index distance caused the six patches | Even with index-distance fallback, mean weights vary by <0.4% across parents; parent 28 is highest (1.008) but is **not** one of the six flux anomalies. Replaced with angular nearest-neighbour regardless. |
| Shard merge / partition aggregation bug | Merge determinism tests pass; anomaly morphology follows equatorial NSIDE=2 parent geometry from mislabelled accumulation, not shard boundaries. |

## Fix

1. **Accumulation:** `gaia_source_id_galactic_nested_pixel()` — level-12 equatorial
   pixel centre → ICRS→Galactic transform → Galactic nested pixel at output `nside`.
2. **Selection lookup:** `gaia_source_id_equatorial_nested_pixel()` — unchanged
   frame for Cantat-Gaudin artifact cells.
3. **Artifact contract:** `coordinate_frame` and `ordering` fields on selection
   artifacts (default `equatorial` / `nested`).
4. **Sparse fallback:** angular separation on the sphere, not `healpix.abs_diff`.
5. **Regression tests:** `starlight_healpix_semantics.rs`, `healpix.rs`,
   `diagnostics.rs`.

## Regeneration scope (required before closing #116)

The existing candidate and weighted shards **must not** be patched. Full rebuild
from verified GaiaSource + XP inputs is required once the corrected generator is
deployed on the production workspace.

**Reusable inputs:** checksum-verified GaiaSource downloads, XP Continuous,
inventories/CAS receipts, UV artifact, photometric-inference artifact, selection
artifact (`1a3670b56eedaf9f9de0b32f081ccfa2baf741a449cd70c2be37d666101a9711`).

**Not reusable:** partition shards, merged candidate CSV, merge report, runtime
packed assets, release-candidate evidence.

**Command (production workspace required):**

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  dataset starlight update \
  --config crates/nsb-data-tools/config/starlight-production-300-650.ladon.toml
```

## Checksums

| Artifact | SHA-256 |
|----------|---------|
| Legacy candidate (superseded) | `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563` |
| Corrected candidate | pending regeneration |

## #103 status

Human scientific and redistribution review remains **PENDING**. When the
corrected candidate is frozen, update `scientific-review-decision-v1.json` and
`redistribution-review-decision-v1.json` to pin the new candidate SHA-256. Do not
carry forward approval for the superseded digest.
