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

## Phase 4 — Stratified Gaia TAP sampling (2026-07-11)

**Gate:** closed on branch after commit `feat: complete reproducible Gaia starlight sampling`.

| Check | Status |
|-------|--------|
| All jobs classified | 69 jobs (68 `completed_valid`, 1 `error_nonretryable` audit) |
| COMPLETED results recovered | 67/67 stratified CSVs validated |
| HTTP 400 explained | `02_invalid_original` — bare `true` boolean rejected by Gaia ADQL |
| Required strata | 67/67 with 512 rows each |
| Checksums | `phase4.sha256sum` + per-job inventory |
| Deduplicated master | 20 041 unique `source_id` |
| Memberships | 32 768 stratum rows |
| Spatial split | HEALPix nside=64, disjoint train/validation/test |
| Domain coverage | validation+test include blue, very red, faint, plane, centre, poles, seam, crowding, low S/N, partial/G-only/no photometry |
| Population reconciliation | frozen totals in `phase4_inputs.manifest.json` |
| TAP client tests | sync/async, 400 no-retry, 429/503 retry, UWS ERROR, HTML, overflow |

Artifacts live under `$HOME/nsb-data/starlight-gaia-release/missing-flux/phase4_*` (not versioned in git).

Code: `starlight_sampling.rs`, `consolidate_gaia_starlight_samples`, extended `gaia_tap` tests.

## Phase 5 — XP continuous reconstruction (in progress, 2026-07-11)

**Gate:** not closed — batch DataLink download running (~12 198 sources, resume-safe checkpoint).

| Check | Status |
|-------|--------|
| Phase 4 inputs frozen | `phase5_phase4_inputs.snapshot.json` verified against `phase4.sha256sum` |
| GaiaXPy environment | 2.1.4 pinned; 10 calibration CSVs checksummed in `phase5_gaiaxpy_environment.json` |
| Overlap targets | 6 342 sources (`phase5_overlap_targets.csv`) |
| Continuous-only targets | 5 856 sources (`phase5_continuous_only_targets.csv`) |
| Batch XP_CONTINUOUS download | `download_xp_continuous_phase5` + checkpoint resume (in flight) |
| Canonical coefficients | `normalize_xp_continuous_coefficients` (`xp_source_{id}.csv` → canonical) |
| Offline reconstruction | GaiaXPy `reconstruct_and_integrate.py` → 336–650 nm normalized grids |
| Overlap validation | `run_starlight_phase5_overlap_validation` vs canonical catalogue flux |
| Uncertainty inflation | fit on train split only, frozen before test |
| Production gates | flux-weighted bias ≤3%, median bias ≤5%, p95 ≤10%, coverage bands — pending full population |
| Continuous-only contributions | `emit_phase5_continuous_contributions` → `phase5_continuous_only_336_650.csv` |
| Reconciliation | `finalize_starlight_phase5` + `phase5_exclusions.csv` |

Known exclusions (documented, not metric-tuned):

- `4062484362784191744` — overlap target absent from canonical `gaia_dr3_starlight_sources.csv` (catalogue reconciliation).

Partial overlap smoke (20 reconstructed sources at checkpoint): pipeline runs end-to-end; gates not evaluable until download completes.

Artifacts: `$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/` (not versioned in git). Auto-resume pipeline: `tools/starlight-xp-continuous/run_phase5_pipeline.sh`.

Code: `starlight_phase5.rs`, Phase 5 binaries, `gaia_xp_continuous.rs`, GaiaXPy audit tool.

## Residual blockers for PRODUCTION READY

1. **Phase 5 batch download + full overlap/continuous-only validation** (in progress)
2. Trained photometric / UV / selection models with checksum-pinned artifacts (Phases 6–10)
3. Independent validation passing preregistered gates on held-out data
4. Human approval artifacts (missing-flux, redistribution, nside review)
5. Bundled production asset in `crates/nsb/data/manifest.toml`

## Conclusion at audit time

**TECHNICALLY IN PROGRESS** — infrastructure present; integrated product path wired in code; scientific population and production gates not yet satisfied.
