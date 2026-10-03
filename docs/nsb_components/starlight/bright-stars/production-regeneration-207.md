# #207 production regeneration (combined bright-star candidate)

Status: **code and tests land in NSB; full-sky regeneration blocked without
BeegFS / Ladon / checksum-pinned external catalogues**.

Do not fabricate candidate, merge-report, or runtime hashes. The current
Gaia-only production baseline remains the registered `nsb` runtime product
until this regeneration completes and release gates clear.

## Scientific contract implemented in-tree

| Identity | Value |
|---|---|
| Combined artifact model | `starlight-bright-stars-combined-v1` |
| Schema | `2` |
| Product band | `combined-300-650` |
| Measured artifact (unchanged) | `starlight-bright-stars-v1` / schema `1` / `measured-336-650` |
| Spectral reconstruction | `xhip-sptype-ck04-v2-hp-bessell2000-v1` |
| UV construction | Same Hp-scaled CK04 SED integrated over 300–336 nm and 336–650 nm |
| Uncertainty | Fully correlated band components (linear sum); named catalogue groups retained |

Measured-only artifacts still fail closed in combined runs.

## Prerequisites (external)

1. BeegFS paths from `crates/nsb-data-tools/config/starlight-bright-stars-v1.ladon.toml`
2. Existing checksum-pinned spectral model under
   `/mnt/beegfs/valles/nsb-data/bright-stars/`
3. Ladon Slurm account / Gaia inventories used by
   `starlight-production-300-650.ladon.toml`
4. Clean NSB commit on `fix/starlight-production-bright-stars-207` (or successor)

## Step 1 — build combined bright-star artifact

Copy the measured build config and set:

```toml
product_band = "combined-300-650"
```

Then:

```bash
NSB_COMMIT=$(git rev-parse HEAD)
CONFIG=crates/nsb-data-tools/config/starlight-bright-stars-combined-v1.ladon.toml
CONFIG_SHA=$(sha256sum "$CONFIG" | awk '{print $1}')
OUT=/mnt/beegfs/valles/nsb-data/bright-stars/artifacts/combined-207

cargo run -p nsb-data-tools --release -- \
  starlight-bright-stars-build \
  --config "$CONFIG" \
  --expected-config-sha256 "$CONFIG_SHA" \
  --expected-build-commit "$NSB_COMMIT" \
  --output-directory "$OUT"
```

Record `artifact_sha256` from
`$OUT/starlight-bright-stars-combined-v1-build-manifest.json`.

## Step 2 — pin and regenerate the combined Starlight candidate

1. Edit
   `crates/nsb-data-tools/config/starlight-production-300-650-bright-stars.ladon.toml`
   and add the verified `[starlight.bright_star_supplement]` pin.
2. Run the normal Ladon production lifecycle (`update` → `build` → `finalize`
   → validation gates) with that config.
3. Record:
   - NSB commit
   - config SHA-256
   - bright-star artifact SHA-256
   - UV / photometric / selection pins (unchanged unless justified)
   - `starlight_nside128.csv` SHA-256
   - `merge_report.json` SHA-256
   - admitted primary / supplement / excluded / replacement / suppressed counts
   - integrated 300–336 / 336–650 / 300–650 fluxes

## Step 3 — validation and runtime promotion

Only after Step 2 produces real artifacts:

1. NSB map/report validation gates
2. Companion `nsb-validation` cross-implementation comparisons (full + no-XHIP)
3. Release-candidate evidence regeneration (do **not** transfer old human approvals)
4. Pack → `starlight_nside128.release.csv` + sidecar
5. Update `crates/nsb/data/manifest.toml` with verified hashes

## What this PR deliberately does not claim

- New candidate SHA-256 / runtime map SHA-256
- Cross-implementation bias/correlation numbers for the new candidate
- `scientifically_validated = true`
- Closure of remaining Gaia-only residuals (~18% no-XHIP class deficit)

Those require the external regeneration path above.
