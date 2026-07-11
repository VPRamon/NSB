# PR #56 Phase 1 Audit — Starlight Production Foundation

**Branch:** `starlight-production-foundation` @ `154dec7`  
**Base:** `main` @ `b725763` (PR #55 merged)  
**Date:** 2026-07-11

## Executive summary

The foundation PR implements a **fail-closed** approval and validation infrastructure targeting the normative **300–650 nm** integrated Starlight contract, while the active sweep path still built **336–650 nm Gaia XP sampled** maps. Production promotion was structurally impossible: validator spectral contract (336–650) contradicted packer production gates (300–650).

This audit drove Phase 2 alignment (see commits on this branch after audit).

## Checklist (Phase 1 findings → Phase 2 status)

| Item | Finding | Status |
|------|---------|--------|
| Band contract | Dual 336–650 vs 300–650 | **Addressed** — validator/packer/sweep aligned |
| `validate_starlight_map` | Hardcoded `gaia_dr3_xp_photon_radiance_336_650nm_v1` | **Addressed** — integrated contract + XP rejection |
| `sweep_starlight_nside` | Called `build_starlight_map` (XP) | **Addressed** — calls `build_integrated_starlight_product` |
| Missing binaries in Cargo.toml | 3 bins unregistered | **Addressed** |
| `starlight_science` | No pipeline consumer | Open — Phases 5–10 |
| Longitude-wrap thresholds | Validator 10 vs runtime 1 | Open — unify in follow-up |
| Independent reference | Provisional 336–650 internal JSON | Open — requires external 300–650 reference |
| Production asset | Not bundled | Open — Phase 16 |
| Real Gaia pipeline CI | Synthetic only | Partial — `integrated_starlight_pipeline` fixture added |

## Schema / wiring notes

- **Integrated product** emits five artifacts via `build_integrated_starlight_product` (mean, uncertainty, completeness, diagnostics, manifest).
- **Runtime** expects HEALPix v1/v2 with `integrated_ph_cm2_ns_sr`; packer converts integrated mean sidecar → runtime v2 with uncertainties.
- **Approval DAG** requires `band_nm = [300, 650]`, human approvals, and nside sweep schema v2.

## Residual blockers for PRODUCTION READY

1. Normalized contributions for full Gaia population (Phases 4–10)
2. Trained photometric / UV / selection models with checksum-pinned artifacts
3. Independent validation passing preregistered gates on held-out data
4. Human approval artifacts (missing-flux, redistribution, nside review)
5. Bundled production asset in `crates/nsb/data/manifest.toml`

## Conclusion at audit time

**TECHNICALLY IN PROGRESS** — infrastructure present; integrated product path wired in code; scientific population and production gates not yet satisfied.
