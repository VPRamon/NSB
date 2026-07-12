#!/usr/bin/env bash
# Rotating USB bulk loop for issue #47 PR A — one partition per iteration.
set -euo pipefail

NSB_REPO="${NSB_REPO:-/path/to/nsb}"
STARLIGHT_ROOT="${STARLIGHT_ROOT:-$HOME/nsb-data/starlight-gaia-release}"
STARLIGHT_FROZEN_POLICY="${STARLIGHT_FROZEN_POLICY:-$STARLIGHT_ROOT/missing-flux/phase5/phase5_frozen_validation_policy_v1.json}"
STARLIGHT_GAIAXPY_ENV="${STARLIGHT_GAIAXPY_ENV:-$STARLIGHT_ROOT/pilot-xp-continuous-bulk/gaiaxpy_environment.json}"
GAIA_USB_MOUNT="${GAIA_USB_MOUNT:-/path/to/external-storage}"
GAIA_USB_ROOT="${GAIA_USB_ROOT:-$GAIA_USB_MOUNT/nsb-data/gaia-bulk}"
LOG_DIR="${GAIA_USB_ROOT}/logs"
mkdir -p "$LOG_DIR"

SESSION_LOG="$LOG_DIR/bulk_loop_$(date -u +%Y%m%dT%H%M%SZ).log"
exec > >(tee -a "$SESSION_LOG") 2>&1

echo "=== bulk loop start $(date -u -Iseconds) ==="
echo "repo=$NSB_REPO commit=$(git -C "$NSB_REPO" rev-parse HEAD)"

mountpoint -q "$GAIA_USB_MOUNT" || { echo "USB not mounted"; exit 1; }

cd "$NSB_REPO"
ITER=0
while true; do
  ITER=$((ITER + 1))
  echo "--- iteration $ITER $(date -u -Iseconds) ---"
  if ! cargo run --release --locked -p nsb-data-tools \
    --bin run_starlight_xp_continuous_bulk_pipeline -- \
    --skip-rehearsal \
    --skip-resume-test \
    --file-limit 1 \
    --production-row-limit 0 \
    --production-batch-size 500 \
    --frozen-policy "$STARLIGHT_FROZEN_POLICY" \
    --gaiaxpy-environment "$STARLIGHT_GAIAXPY_ENV" \
    --usb-mountpoint "$GAIA_USB_MOUNT" \
    --usb-cache-root "$GAIA_USB_ROOT" \
    --merge-partition-checkpoints \
    --cleanup-verified-inputs \
    --cleanup-limit 1; then
    echo "pipeline iteration $ITER failed; stopping loop"
    exit 1
  fi
  echo "iteration $ITER complete"
done
