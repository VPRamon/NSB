#!/usr/bin/env bash
# Incrementally normalize and reconstruct Phase 5 DataLink downloads without restarting acquisition.
set -euo pipefail

ROOT="${NSB_ROOT:-/home/valles/workspace/nsb}"
PHASE5="${PHASE5_ROOT:-$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5}"
VENV="${ROOT}/tools/starlight-xp-continuous/.venv/bin/python"

cd "$ROOT"

echo "== inspect download (read-only) =="
cargo run --locked -q -p nsb-data-tools --bin inspect_phase5_download -- \
  --phase5-root "$PHASE5" \
  --targets-csv "$PHASE5/phase5_all_xp_continuous_targets.csv"

echo "== normalize new raw coefficients =="
mkdir -p "$PHASE5/coefficients/canonical"
cargo run --locked -q -p nsb-data-tools --bin normalize_xp_continuous_coefficients -- \
  --raw-dir "$PHASE5/coefficients/raw" \
  --output-dir "$PHASE5/coefficients/canonical" \
  --manifest-json "$PHASE5/phase5_coefficients.manifest.json"

echo "== Rust in-process reconstruction (resume via manifest) =="
mkdir -p "$PHASE5/reconstruction/normalized"
cargo run --release --locked -q -p nsb-data-tools --bin reconstruct_canonical_coefficients -- \
  --coefficients-dir "$PHASE5/coefficients/canonical" \
  --output-dir "$PHASE5/reconstruction/normalized" \
  --manifest "$PHASE5/phase5_reconstruction.manifest.json" \
  --gaiaxpy-environment "$PHASE5/phase5_gaiaxpy_environment.json"

echo "== incremental Phase 5 processing complete =="
