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
| Prefix schema inspection | `crates/nsb-data-tools/src/gaia_xp_continuous_bulk_schema.rs` |
| Bulk file index | `index_gaia_xp_continuous_bulk` |
| Multifile pilot | `run_phase5b_multifile_pilot` |
| Streaming mini-pilot | `run_phase5b_mini_pilot` |
| Chunk benchmark | `run_phase5b_chunk_benchmark` |
| Merge / resume validation | `run_phase5b_merge_validation`, `run_phase5b_resume_validation` |
| HEALPix accumulator | `crates/nsb-data-tools/src/gaia_xp_continuous_healpix.rs` |

## Phase 5B multifile pilot (2026-07-11)

**PHASE 5B MULTIFILE PILOT PASSED — READY FOR SCIENTIFIC POLICY**

Two downloaded prefixes (~2.75 GiB compressed):

```text
XpContinuousMeanSpectrum_000000-003111.csv.gz
XpContinuousMeanSpectrum_003112-005263.csv.gz
```

| Gate | Result |
| --- | --- |
| Same adapter / schema on both prefixes | PASS — identical headers, `bp_n_parameters=55`, 1485 correlations |
| Multifile streaming (20,000 rows / 19,995 valid) | PASS — peak RSS ~50 MiB |
| Per-file reconciliation | PASS — all exclusions logged (`non_positive_flux`) |
| Resume (10k uninterrupted vs 5k+5k) | PASS |
| Merge order independence (1→2 vs 2→1) | PASS — checksum `a875a1b0d0c302c…` |
| Single-worker vs multi-worker merge | PASS |
| Bulk index + row verification | PASS — both prefixes |
| Chunk benchmark | **500** selected (100 / 500 / 1000) |
| Resource estimate (184,729,270 sources) | ~44 d (1 worker), ~11 d (4 workers) at ~41.6 src/s |

This validates **operational scalability** only. Final scientific policy still
requires Phase 5 DataLink overlap train/validation/test and
`phase5_frozen_validation_policy.json`.

Evidence: `~/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/phase5b_multifile_*`

Multifile pilot example:

```bash
cargo run --locked -p nsb-data-tools --bin run_phase5b_multifile_pilot -- \
  --bulk-dir ~/nsb-data/.../bulk \
  --output-dir ~/nsb-data/.../multifile_pilot \
  --row-limit 10000 \
  --batch-size 500 \
  --gaiaxpy-environment ~/nsb-data/.../gaiaxpy_environment.json
```

Index locate with row verification:

```bash
cargo run --locked -p nsb-data-tools --bin index_gaia_xp_continuous_bulk -- \
  --download-dir bulk --output-dir ~/nsb-data/.../ \
  locate --source-id 4295806720 --verify-row
```

## Phase 5 DataLink (parallel)

12,198-target stratified sample uses DataLink with a **single** resumable
downloader. Do not start a second downloader. Bulk production must wait for
`phase5_frozen_validation_policy.json` after overlap validation on test.

## Resource estimates (184,729,270 XP continuous-only sources)

From multifile pilot stable throughput (~41.6 valid sources/s, batch 500,
peak RSS ~50 MiB per worker):

| Scenario | Wall time (reconstruction) |
| --- | --- |
| 1 worker | ~44 days |
| 4 workers | ~11 days |
| 8 workers | ~5.5 days |
| Bulk download (3.3 TiB @ 40 MiB/s) | ~24 h (transfer only) |

Do **not** launch full-population processing until Phase 5 scientific policy is
frozen and verified on the DataLink validation sample.

## USB rotating cache (issue #47 PR A)

Production bulk uses a **rotating USB cache** on vfat-safe file sizes (≤ 4 GiB
per file). Official coefficient files are downloaded to USB, processed on
internal storage, checkpointed, then deleted from USB once marked **releasable**.

### Layout

| Path | Purpose |
| --- | --- |
| `$GAIA_USB_ROOT/.nsb-gaia-cache-root.json` | Cache root marker (UUID) |
| `$GAIA_USB_ROOT/xp-continuous/` | Rotating `.csv.gz` inputs |
| `$GAIA_USB_ROOT/manifests/cache_state_manifest.json` | Per-file cache state machine |
| `$GAIA_USB_ROOT/manifests/storage_plan.json` | Preflight storage plan |
| `$STARLIGHT_CHECKPOINTS/` | Per-file HEALPix accumulator checkpoints |

Set via environment (no hardcoded home paths in library code):

```bash
export GAIA_USB_MOUNT=/path/to/external-storage
export GAIA_USB_ROOT=$GAIA_USB_MOUNT/nsb-data/gaia-bulk
export GAIA_USB_CACHE=$GAIA_USB_ROOT/xp-continuous
export GAIA_USB_MANIFESTS=$GAIA_USB_ROOT/manifests
export STARLIGHT_WORK=$HOME/nsb-data/starlight-gaia-release/xp-continuous-bulk/work
export STARLIGHT_CHECKPOINTS=$HOME/nsb-data/starlight-gaia-release/xp-continuous-bulk/checkpoints
export STARLIGHT_OUTPUTS=$HOME/nsb-data/starlight-gaia-release/xp-continuous-bulk/outputs
```

### Cache state machine

```text
planned → downloading → downloaded → checksum_verified
  → processing → processed → output_verified → releasable → deleted
failed (retry from planned/download)
```

### Orchestrator (`run_starlight_xp_continuous_bulk_pipeline`)

Preflight, rehearsal, verified-cache processing, per-file production loop, and
input cleanup:

```bash
cargo run --locked -p nsb-data-tools --bin run_starlight_xp_continuous_bulk_pipeline -- \
  --preflight-only \
  --usb-mountpoint "$GAIA_USB_MOUNT" \
  --usb-cache-root "$GAIA_USB_ROOT"
```

| Flag | Purpose |
| --- | --- |
| `--preflight-only` | Storage plan + inventory audit only (skip rehearsal) |
| `--skip-rehearsal` | Skip representative mini-pilot and resume test |
| `--skip-resume-test` | Skip kill/resume validation |
| `--process-verified-cache-limit N` | Process N `checksum_verified` files → `releasable` |
| `--production-row-limit N` | Production streaming row cap (`0` = entire partition file) |
| `--production-batch-size` | GaiaXPy batch size for production streaming (default 500) |
| `--file-limit N` | Per-file production loop (download → process → HEALPix checkpoint → releasable) |
| `--cleanup-verified-inputs` | Delete `releasable` inputs from USB cache |
| `--cleanup-limit N` | Live cleanup: delete at most N releasable files |
| `--dry-run` | With cleanup: list candidates without deleting |
| `--cache-subdir xp-continuous` | USB cache subdirectory (default) |
| `--max-cache-bytes` | USB cache footprint cap (default 20 GiB) |
| `--init-usb-marker` | Create `.nsb-gaia-cache-root.json` on first use |

Controlled cleanup (1 file live delete):

```bash
cargo run --locked -p nsb-data-tools --bin run_starlight_xp_continuous_bulk_pipeline -- \
  --skip-rehearsal --cleanup-verified-inputs --cleanup-limit 1 \
  --usb-mountpoint "$GAIA_USB_MOUNT" --usb-cache-root "$GAIA_USB_ROOT"
```

Production loop skeleton (1 file):

```bash
cargo run --locked -p nsb-data-tools --bin run_starlight_xp_continuous_bulk_pipeline -- \
  --skip-rehearsal --file-limit 1 \
  --usb-mountpoint "$GAIA_USB_MOUNT" --usb-cache-root "$GAIA_USB_ROOT" \
  --gaiaxpy-environment "$STARLIGHT_ROOT/pilot-xp-continuous-bulk/gaiaxpy_environment.json"
```

### Downloader (`download_gaia_xp_continuous_bulk`)

USB cache mode wires downloads into the state machine:

```bash
cargo run --locked -p nsb-data-tools --bin download_gaia_xp_continuous_bulk -- \
  --usb-mountpoint "$GAIA_USB_MOUNT" \
  --usb-cache-root "$GAIA_USB_ROOT" \
  --file-limit 4
```

| Flag | Purpose |
| --- | --- |
| `--file-limit N` | Download first N pending inventory files |
| `--only-filename NAME` | Download a single named file |
| `--resume` | Resume partial `.part` downloads |
| `--report-json PATH` | Write combined bulk + cache sync report |

Reports land under `$GAIA_USB_ROOT/manifests/` (e.g.
`gaia_xp_continuous_usb_cache_download_report.json`,
`cleanup_simulation.json`, `pipeline_report.json`).

### Reconciliation scaffold

Per-partition manifests and a rolling ledger are written under
`$GAIA_USB_RECONCILIATION/` (default: `$GAIA_USB_ROOT/reconciliation`):

| Artifact | Purpose |
| --- | --- |
| `{partition}.reconciliation.json` | Per-file valid/excluded/failed counts + HEALPix totals |
| `bulk_reconciliation_ledger.json` | Cumulative partition accounting (184.7M close deferred) |

Production streaming uses `run_phase5b_mini_pilot` (canonical adapter +
GaiaXPy + HEALPix) with `--production-row-limit 0` for full partition files.
Rehearsal uses the same binary with a bounded `--rehearsal-row-limit`.
