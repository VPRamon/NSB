#!/usr/bin/env bash
# Fetch holdout v1 TAP results (21 strata, async).
set -euo pipefail

ROOT="${NSB_ROOT:-/path/to/nsb}"
HOLDOUT="${HOLDOUT_ROOT:-$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5/holdout_v1}"
JOBS="$HOLDOUT/jobs"
RESULTS="$HOLDOUT/results"
mkdir -p "$RESULTS"

cd "$ROOT"
for adql in "$JOBS"/holdout_v1_*.adql; do
  base="$(basename "$adql" .adql)"
  out="$RESULTS/${base}.csv"
  if [[ -s "$out" ]]; then
    echo "skip existing $base"
    continue
  fi
  echo "== TAP $base =="
  cargo run --locked -q -p nsb-data-tools --bin query_gaia_tap -- \
    --query-file "$adql" \
    --mode async \
    --format csv \
    --output "$out" \
    --artifacts-dir "$RESULTS/${base}.tap-artifacts"
done

echo "holdout TAP fetch complete -> $RESULTS"
