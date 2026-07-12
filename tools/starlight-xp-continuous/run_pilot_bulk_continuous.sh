#!/usr/bin/env bash
# Phase 5B pilot: download a small XP continuous bulk prefix and benchmark streaming reconstruction.
set -euo pipefail

ROOT="${NSB_ROOT:-/path/to/nsb}"
PILOT_ROOT="${PILOT_ROOT:-$HOME/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk}"
VENV="${ROOT}/tools/starlight-xp-continuous/.venv/bin/python"
FILE_LIMIT="${FILE_LIMIT:-3}"
ROW_LIMIT="${ROW_LIMIT:-128}"
BATCH_SIZE="${BATCH_SIZE:-64}"
WORKERS="${WORKERS:-2}"

cd "$ROOT"

mkdir -p "$PILOT_ROOT"

echo "== Download official XP continuous bulk prefix (file_limit=${FILE_LIMIT}) =="
cargo run --locked -q -p nsb-data-tools --bin download_gaia_xp_continuous_bulk -- \
  --download-dir "$PILOT_ROOT/bulk" \
  --file-limit "$FILE_LIMIT" \
  --resume \
  --report-json "$PILOT_ROOT/bulk_download_report.json"

echo "== GaiaXPy environment audit =="
"$VENV" "$ROOT/tools/starlight-xp-continuous/audit_gaiaxpy_environment.py" \
  --output-json "$PILOT_ROOT/gaiaxpy_environment.json" \
  --output-sha256 "$PILOT_ROOT/gaiaxpy_environment.sha256"

BULK_GZ="$(find "$PILOT_ROOT/bulk" -maxdepth 1 -name 'XpContinuousMeanSpectrum_*.csv.gz' | sort | head -1)"
if [[ -z "$BULK_GZ" ]]; then
  echo "no bulk gzip under $PILOT_ROOT/bulk"
  exit 1
fi

echo "== Streaming bulk reconstruction pilot (Rust in-process) =="
mkdir -p "$PILOT_ROOT/reconstruction"
cargo run --release --locked -q -p nsb-data-tools --bin run_phase5b_mini_pilot -- \
  --bulk-gz "$BULK_GZ" \
  --output-dir "$PILOT_ROOT/reconstruction" \
  --row-limit "$ROW_LIMIT" \
  --batch-size "$BATCH_SIZE" \
  --workers "$WORKERS" \
  --gaiaxpy-environment "$PILOT_ROOT/gaiaxpy_environment.json" \
  --resume

echo "Pilot complete -> $PILOT_ROOT"
