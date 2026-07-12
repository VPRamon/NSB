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

## Reconstruct canonical coefficient CSVs (Phase 5 / holdout)

```bash
cargo run --release --locked -p nsb-data-tools --bin reconstruct_canonical_coefficients -- \
  --coefficients-dir "$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/coefficients/canonical" \
  --output-dir "$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/reconstruction/normalized" \
  --manifest "$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/phase5_reconstruction.manifest.json"
```

## Bulk production (336–650 nm continuous-only)

```bash
PRODUCTION_WORKERS=18 bash tools/starlight-xp-continuous/run_bulk_until_shutdown.sh
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
