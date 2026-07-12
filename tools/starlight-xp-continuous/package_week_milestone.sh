#!/usr/bin/env bash
# Package honest 336–650 nm XP continuous milestone candidate (issue #47 subset).
set -euo pipefail

NSB_REPO="${NSB_REPO:-/path/to/nsb}"
STARLIGHT_ROOT="${STARLIGHT_ROOT:-$HOME/nsb-data/starlight-gaia-release}"
STARLIGHT_CHECKPOINTS="${STARLIGHT_CHECKPOINTS:-$STARLIGHT_ROOT/checkpoints}"
GAIA_USB_ROOT="${GAIA_USB_ROOT:-/path/to/external-storage/nsb-data/gaia-bulk}"
WORK_DIR="${WORK_DIR:-$STARLIGHT_ROOT/work/milestone_pack}"
CANDIDATE_DIR="${CANDIDATE_DIR:-$STARLIGHT_ROOT/candidates/xp_continuous_336_650_week1}"

mkdir -p "$WORK_DIR" "$CANDIDATE_DIR"
cd "$NSB_REPO"

cargo build --release --locked -p nsb-data-tools \
  --bin run_starlight_xp_continuous_bulk_pipeline \
  --bin export_starlight_healpix_to_contributions \
  --bin pack_starlight_asset

cargo run --release --locked -p nsb-data-tools \
  --bin run_starlight_xp_continuous_bulk_pipeline -- \
  --work-dir "$WORK_DIR" \
  --checkpoint-dir "$STARLIGHT_CHECKPOINTS" \
  --usb-cache-root "$GAIA_USB_ROOT" \
  --merge-partition-checkpoints \
  --backfill-reconciliation \
  --skip-rehearsal \
  --skip-resume-test \
  --preflight-only

HEALPIX_ACC="$STARLIGHT_CHECKPOINTS/bulk_healpix_accumulator.json"
CONTRIBUTIONS="$CANDIDATE_DIR/xp_continuous_336_650_contributions.parquet"

cargo run --release --locked -p nsb-data-tools \
  --bin export_starlight_healpix_to_contributions -- \
  --accumulator "$HEALPIX_ACC" \
  --output "$CONTRIBUTIONS" \
  --branch xp_continuous \
  --effective-measured-band-nm 336,650

cargo run --release --locked -p nsb-data-tools \
  --bin pack_starlight_asset -- \
  --contributions "$CONTRIBUTIONS" \
  --output-dir "$CANDIDATE_DIR" \
  --candidate \
  --label xp_continuous_336_650_week1

echo "milestone candidate staged under $CANDIDATE_DIR"
