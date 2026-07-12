#!/usr/bin/env bash
# Phase 5B pilot: download a small XP continuous bulk prefix and benchmark streaming reconstruction.
set -euo pipefail

ROOT="${NSB_ROOT:-/path/to/nsb}"
PILOT_ROOT="${PILOT_ROOT:-$HOME/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk}"
VENV="${ROOT}/tools/starlight-xp-continuous/.venv/bin/python"
FILE_LIMIT="${FILE_LIMIT:-3}"
ROW_LIMIT="${ROW_LIMIT:-128}"

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

echo "== Streaming bulk reconstruction pilot =="
"$VENV" "$ROOT/tools/starlight-xp-continuous/pilot_bulk_continuous.py" \
  --bulk-dir "$PILOT_ROOT/bulk" \
  --checkpoint "$PILOT_ROOT/reconstruction_checkpoint.jsonl" \
  --report-json "$PILOT_ROOT/pilot_report.json" \
  --file-limit "$FILE_LIMIT" \
  --row-limit "$ROW_LIMIT" \
  --resume

echo "Pilot complete -> $PILOT_ROOT"
