# Gaia DR3 XP continuous offline reconstruction (Phase 5 + bulk)

Production calibration runs **in-process in Rust** (`gaia_xp_continuous_calibrate`).
Python + pinned GaiaXPy 2.1.4 are used only for **environment audit** and **parity oracle fixtures**.

## Layout

```text
$HOME/nsb-data/starlight-gaia-release/missing-flux/xp-continuous/
  coefficients/raw/          # XP_CONTINUOUS DataLink CSV (via query_gaia / datalink)
  reconstruction/normalized/ # calibrated 336–650 nm grids (NSB normalized CSV)
  validation/                # overlap-sample bias reports
```

## Setup (oracle / audit only)

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

## Bulk production (336–650 nm continuous-only)

Prefer local NVMe for `--work-dir` / `--checkpoint-dir` on many-core hosts; USB
rotating cache remains supported with `PRODUCTION_PARALLEL_PARTITIONS=1`.

```bash
# Workstation / USB (default packing: 1 partition, auto workers = cores - headroom)
bash tools/starlight-xp-continuous/run_bulk_until_shutdown.sh

# Explicit workers
PRODUCTION_WORKERS=18 bash tools/starlight-xp-continuous/run_bulk_until_shutdown.sh

# Cluster-style packing (example: 8 partitions × auto workers on a large node)
PRODUCTION_PARALLEL_PARTITIONS=8 \
PRODUCTION_WORKER_HEADROOM=2 \
PRODUCTION_CHECKPOINT_INTERVAL=0 \
  bash tools/starlight-xp-continuous/run_bulk_until_shutdown.sh
```

Worker packing: `parallel_partitions * workers_per_partition ≈ available_cores - headroom`.
Partition claims under `$STARLIGHT_CHECKPOINTS/claims/` prevent double-processing across
processes. Global HEALPix merge takes an exclusive flock.

## Reconstruct canonical coefficient CSVs (Phase 5 / holdout)

```bash
cargo run --release --locked -p nsb-data-tools --bin nsb-data -- \
  starlight xp-continuous reconstruct \
  --coefficients-dir "$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/coefficients/canonical" \
  --output-dir "$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/reconstruction/normalized" \
  --manifest "$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/phase5_reconstruction.manifest.json"
```

## Oracle tooling (CI parity gate)

```bash
# Export design matrices once (GaiaXPy 2.1.4 pinned venv)
.venv/bin/python export_gaiaxpy_design_matrices.py \
  --output crates/nsb-data-tools/tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json

# Freeze GaiaXPy oracle fixtures from bulk sample
.venv/bin/python generate_gaiaxpy_oracle_fixtures.py \
  --bulk-gz /path/to/XpContinuousMeanSpectrum_*.csv.gz \
  --output crates/nsb-data-tools/tests/fixtures/gaiaxpy_oracle/continuous_parity_v1.json

cargo test -p nsb-data-tools gaia_xp_continuous_calibrate_parity
```

## Validate against XP sampled (overlap population)

Use `validate_xp_continuous_reconstruction` in `nsb-data-tools` after both
continuous reconstruction and sampled DataLink products exist for overlap sources.
