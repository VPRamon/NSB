# Gaia DR3 XP continuous bulk processing (Phase 5B)

## Official product

Gaia DR3 exposes XP continuous spectra as **basis-function coefficients**, not
pre-sampled flux arrays. The authoritative bulk delivery is:

| Field | Value |
| --- | --- |
| Table | `xp_continuous_mean_spectrum` |
| Bulk URL | https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_continuous_mean_spectrum/ |
| Checksum manifest | `_MD5SUM.txt` (MD5, official ESA) |
| Partition files | 3,386 `XpContinuousMeanSpectrum_*.csv.gz` |
| Estimated compressed size | ~3.3 TiB |
| TAP access | **Not available** — Massive Data / CDN only |
| License / credit | Gaia DR3 — https://gea.esac.esa.int/archive/documentation/credits.html |

Primary documentation:

- [xp_continuous_mean_spectrum datamodel](https://gea.esac.esa.int/archive/documentation/GDR3/Gaia_archive/chap_datamodel/sec_dm_spectroscopic_tables/ssec_dm_xp_continuous_mean_spectrum.html)
- [Gaia archive programmatic access](https://www.cosmos.esa.int/web/gaia-users/archive/programmatic-access)

Each bulk row contains `source_id`, BP/RP coefficient arrays, coefficient
errors, and quality metadata. NSB reconstructs spectra offline with pinned
GaiaXPy 2.1.4 and integrates the inclusive **336–650 nm** photon-flux band.
This is **not** labelled as the integrated 300–650 nm product until the
independently validated 300–336 nm correction (Phase 7) is applied.

## Mandatory strategy

Individual DataLink requests for 184,729,270 continuous-only sources are
forbidden. The production path is:

```text
official bulk coefficient files
  → resumable MD5-validated download
  → streaming ECSV batch read
  → GaiaXPy reconstruction (pinned 2.1.4)
  → integrate 336–650 nm
  → apply frozen Phase 5 quality policy
  → accumulate HEALPix (mean, variance, counts)
  → checkpoint per bulk file / batch
  → discard transient coefficient batches
```

Persistent outputs are manifests, checksums, batch accounting, rejected rows,
HEALPix accumulators, and reproducible commands — not a 184M-row CSV.

## NSB tooling

| Component | Path |
| --- | --- |
| Bulk downloader | `download_gaia_xp_continuous_bulk` |
| Canonical adapter | `crates/nsb-data-tools/src/gaia_xp_continuous_canonical.rs` |
| Bulk file index | `index_gaia_xp_continuous_bulk` |
| Bulk/DataLink cross-check | `run_phase5b_cross_comparison` |
| Streaming mini-pilot | `run_phase5b_mini_pilot` |
| Chunk benchmark | `run_phase5b_chunk_benchmark` |
| Merge validation | `run_phase5b_merge_validation` |
| Resume validation | `run_phase5b_resume_validation` |
| HEALPix accumulator | `crates/nsb-data-tools/src/gaia_xp_continuous_healpix.rs` |
| Bulk index library | `crates/nsb-data-tools/src/gaia_xp_continuous_bulk_index.rs` |
| Shared bulk engine | `crates/nsb-data-tools/src/gaia_bulk.rs` |
| GaiaXPy flux validation | `tools/starlight-xp-continuous/phase5b_gaiaxpy_flux_validate.py` |
| Schema audit emitter | `tools/starlight-xp-continuous/emit_phase5b_schema_artifacts.py` |

## Phase 5B pilot status (2026-07-11)

Operational mini-pilot on prefix file `XpContinuousMeanSpectrum_000000-003111.csv.gz`
(~1.33 GiB compressed). **This is throughput/resume validation, not the final
scientific overlap gate** (that remains Phase 5 DataLink sample acquisition).

| Gate | Result |
| --- | --- |
| Canonical adapter 4/4 bulk ↔ DataLink | PASS — task 306212 superseded |
| Bulk ECSV schema (`bp_n_parameters=55`, 1485 correlations) | PASS |
| Mini-pilot streaming (10,000 rows / 9,998 valid) | PASS — ~49.5 sources/s, peak RSS ~49 MiB |
| Full-file RAM avoidance | PASS — streaming gzip ECSV |
| HEALPix accumulation (`nside=64`) | PASS — checksum `230556b6947732ec…` |
| Resume (5,000 + 5,000 rows) | PASS — identical HEALPix and flux checksums |
| Multi-worker merge (5,000 ∥ 5,000) | PASS — identical to single-worker reference |
| Chunk benchmark (100 / 500 / 1000) | PASS — selected batch size **500** |
| Bulk file index (3,386 files) | PASS — `source_id → bulk file` routing |
| Second prefix smoke test | PASS — `XpContinuousMeanSpectrum_003112-005263.csv.gz` |
| Reconciliation (`valid + excluded + failed = handled`) | PASS — 2 non-positive flux exclusions logged |
| Resource estimate (184,729,270 sources) | PASS — ~43 d (1 worker), ~11 d (4 workers) at stable pilot rate |

Evidence artifacts (outside repo):
`~/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/phase5b_*.{json,csv,md}`.

Architecture:

```text
bulk ECSV row ──→ parse_bulk_ecsv_record ──→ CanonicalXpContinuousRecord
                                                      │
                                                      ▼
                                        write_gaiaxpy_datalink_csv(_batch)
                                                      │
                                                      ▼
                                           GaiaXPy 2.1.4 → integrate 336–650 nm
                                                      │
                                                      ▼
                              XpContinuousHealpixAccumulator (checkpoint / merge)
```

Mini-pilot example:

```bash
cargo run --locked -p nsb-data-tools --bin run_phase5b_mini_pilot -- \
  --bulk-gz ~/nsb-data/.../XpContinuousMeanSpectrum_000000-003111.csv.gz \
  --output-dir ~/nsb-data/.../mini_pilot_run \
  --row-limit 10000 \
  --batch-size 500 \
  --gaiaxpy-environment ~/nsb-data/.../gaiaxpy_environment.json \
  --skip-normalized-output
```

Index example:

```bash
cargo run --locked -p nsb-data-tools --bin index_gaia_xp_continuous_bulk -- \
  --md5-manifest bulk/_MD5SUM.txt \
  --download-dir bulk \
  --output-dir ~/nsb-data/.../ \
  locate --source-id 4295806720
```

## Resource estimates (184,729,270 XP continuous-only sources)

From stable mini-pilot throughput (~49.5 valid sources/s, ~6.8 MiB/s read,
peak RSS ~50 MiB per worker, batch size 500):

| Scenario | Wall time (reconstruction only) | Notes |
| --- | --- | --- |
| 1 worker | ~43 days | CPU-bound GaiaXPy |
| 4 workers | ~11 days | one bulk file per worker, deterministic merge |
| 8 workers | ~5.4 days | RAM-safe; avoid oversubscribing GaiaXPy |
| Bulk download (3.3 TiB @ 40 MiB/s) | ~24 h | transfer only, resumable MD5 |

Checkpoint storage is O(active pixels) per worker; transient disk is one
coefficient batch CSV per chunk (~500 rows), not full spectra.

## Coverage and fallback

If official bulk coverage for the frozen 184,729,270 continuous-only population
is incomplete after inventory reconciliation, the deficit must be:

1. recorded exactly in population reconciliation JSON, and
2. filled with the validated photometric branch policy (Phase 6), not silent
   duplication or DataLink fan-out.

## Phase 5 sample vs bulk

Phase 5 overlap/continuous-only **sample** acquisition (12,198 targets) uses
DataLink with checkpoint resume for validation against the XP sampled canonical
catalogue. That path is independent of bulk processing and must not be restarted
while an active downloader is advancing. The bulk pipeline must verify
`phase5_frozen_validation_policy.json` before production scale-up once Phase 5
overlap validation completes.
