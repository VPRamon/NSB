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
| Bulk/DataLink cross-check | `run_phase5b_cross_comparison` |
| Streaming mini-pilot | `run_phase5b_mini_pilot` |
| Shared bulk engine | `crates/nsb-data-tools/src/gaia_bulk.rs` |
| GaiaXPy flux validation | `tools/starlight-xp-continuous/phase5b_gaiaxpy_flux_validate.py` |
| Schema audit emitter | `tools/starlight-xp-continuous/emit_phase5b_schema_artifacts.py` |
| Pilot orchestrator | `tools/starlight-xp-continuous/run_pilot_bulk_continuous.sh` |
| Pilot wrapper | `tools/starlight-xp-continuous/pilot_bulk_continuous.py` |

## Phase 5B pilot status (2026-07-11)

Pilot gate on prefix file `XpContinuousMeanSpectrum_000000-003111.csv.gz`:

| Gate | Result |
| --- | --- |
| Bulk ECSV streaming parser | PASS — `csv` reader with `#` comment skip, 512 KiB buffer |
| Canonical bulk ↔ DataLink equivalence (4 overlap sources) | PASS — max abs diff 0.0 on coefficients, errors, correlations |
| GaiaXPy 2.1.4 accepts bulk-derived CSV | PASS — 4/4 sources calibrate |
| 336–650 nm flux equivalence bulk vs DataLink | PASS — relative flux diff ≤ 1e-8 |
| Mini-pilot streaming (1,000 sources) | PASS — ~37 sources/s, no full-file RAM load |
| Resume correctness | PASS — identical flux checksum interrupted vs uninterrupted |

Evidence artifacts (outside repo): `~/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/phase5b_*.{json,csv,md}`.

Architecture:

```text
bulk ECSV row ──→ parse_bulk_ecsv_record ──→ CanonicalXpContinuousRecord ←── parse_datalink_gaiaxpy_csv
                                                      │
                                                      ▼
                                        write_gaiaxpy_datalink_csv(_batch)
                                                      │
                                                      ▼
                                           GaiaXPy 2.1.4 calibrate → integrate 336–650 nm
```

Download example:

```bash
cargo run --locked -p nsb-data-tools --bin download_gaia_xp_continuous_bulk -- \
  --download-dir ~/nsb-data/starlight-gaia-release/gaia_dr3_xp_continuous_bulk \
  --resume
```

Pilot example (representative prefix, restartable):

```bash
FILE_LIMIT=3 ROW_LIMIT=128 \
  tools/starlight-xp-continuous/run_pilot_bulk_continuous.sh
```

## Resource estimates (pre-full-run)

From CDN metadata and sampled file sizes:

| Metric | Estimate |
| --- | --- |
| Inventory files | 3,386 |
| Compressed total | ~3.3 TiB |
| Transfer at 40 MiB/s | ~24 h (transfer only) |
| Per-file compressed size | ~0.2–1.5 GiB typical |
| Reconstruction | CPU-bound; pilot measures sources/s and peak RSS |

Full-population duration is recorded by the pilot report
(`estimated_full_population_seconds`) after a representative run. Until the
pilot completes on multiple bulk files with restart/resume, treat throughput
numbers as **candidate** rather than production commitments.

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
while an active downloader is advancing.
