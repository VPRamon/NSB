#!/usr/bin/env bash
# Rotating USB (or local-cache) bulk loop for issue #47 — claim-locked partitions.
# Defaults stay single-partition for USB. Raise PRODUCTION_PARALLEL_PARTITIONS on
# many-core hosts (prefer NVMe --work-dir / --checkpoint-dir).
set -euo pipefail

NSB_REPO="${NSB_REPO:-/path/to/nsb}"
STARLIGHT_ROOT="${STARLIGHT_ROOT:-$HOME/nsb-data/starlight-gaia-release}"
STARLIGHT_WORK="${STARLIGHT_WORK:-$STARLIGHT_ROOT/work}"
STARLIGHT_CHECKPOINTS="${STARLIGHT_CHECKPOINTS:-$STARLIGHT_ROOT/checkpoints}"
STARLIGHT_FROZEN_POLICY="${STARLIGHT_FROZEN_POLICY:-$STARLIGHT_ROOT/missing-flux/phase5/phase5_frozen_validation_policy_v1.json}"
STARLIGHT_GAIAXPY_ENV="${STARLIGHT_GAIAXPY_ENV:-$STARLIGHT_ROOT/pilot-xp-continuous-bulk/gaiaxpy_environment.json}"
GAIA_USB_MOUNT="${GAIA_USB_MOUNT:-/path/to/external-storage}"
GAIA_USB_ROOT="${GAIA_USB_ROOT:-$GAIA_USB_MOUNT/nsb-data/gaia-bulk}"
GAIA_USB_MANIFESTS="${GAIA_USB_MANIFESTS:-$GAIA_USB_ROOT/manifests}"
GAIA_USB_RECONCILIATION="${GAIA_USB_RECONCILIATION:-$GAIA_USB_ROOT/reconciliation}"
LOG_DIR="${GAIA_USB_ROOT}/logs"
mkdir -p "$LOG_DIR" "$STARLIGHT_WORK" "$STARLIGHT_CHECKPOINTS"

# Retry on transient failures (CDN/network). 0 = retry indefinitely.
BULK_LOOP_MAX_RETRIES="${BULK_LOOP_MAX_RETRIES:-0}"
BULK_LOOP_RETRY_BASE_SEC="${BULK_LOOP_RETRY_BASE_SEC:-60}"
BULK_LOOP_RETRY_MAX_SEC="${BULK_LOOP_RETRY_MAX_SEC:-1800}"

# Packing: parallel_partitions * workers_per_partition ≈ cores - headroom
PRODUCTION_PARALLEL_PARTITIONS="${PRODUCTION_PARALLEL_PARTITIONS:-1}"
PRODUCTION_WORKERS="${PRODUCTION_WORKERS:-0}"
PRODUCTION_WORKER_HEADROOM="${PRODUCTION_WORKER_HEADROOM:-0}"
PRODUCTION_CHECKPOINT_INTERVAL="${PRODUCTION_CHECKPOINT_INTERVAL:-0}"
PRODUCTION_BATCH_SIZE="${PRODUCTION_BATCH_SIZE:-1000}"

# Export so nested tools never fall back to cwd="."
export STARLIGHT_WORK STARLIGHT_CHECKPOINTS STARLIGHT_FROZEN_POLICY STARLIGHT_GAIAXPY_ENV
export GAIA_USB_MOUNT GAIA_USB_ROOT GAIA_USB_MANIFESTS GAIA_USB_RECONCILIATION
export PRODUCTION_WORKERS PRODUCTION_WORKER_HEADROOM PRODUCTION_CHECKPOINT_INTERVAL

SESSION_LOG="$LOG_DIR/bulk_loop_$(date -u +%Y%m%dT%H%M%SZ).log"
exec > >(tee -a "$SESSION_LOG") 2>&1

echo "=== bulk loop start $(date -u -Iseconds) ==="
echo "repo=$NSB_REPO commit=$(git -C "$NSB_REPO" rev-parse HEAD)"
echo "work=$STARLIGHT_WORK checkpoints=$STARLIGHT_CHECKPOINTS"
echo "retry_policy=max_retries=${BULK_LOOP_MAX_RETRIES:-0} (0=unlimited) base_sec=${BULK_LOOP_RETRY_BASE_SEC} max_sec=${BULK_LOOP_RETRY_MAX_SEC}"
echo "packing=parallel_partitions=${PRODUCTION_PARALLEL_PARTITIONS} workers=${PRODUCTION_WORKERS} (0=auto) headroom=${PRODUCTION_WORKER_HEADROOM} (0=auto) checkpoint_interval=${PRODUCTION_CHECKPOINT_INTERVAL} (0=auto)"

mountpoint -q "$GAIA_USB_MOUNT" || { echo "USB not mounted"; exit 1; }

cd "$NSB_REPO"

cargo build --release --locked -p nsb-data-tools \
  --bin run_starlight_xp_continuous_bulk_pipeline \
  --bin run_phase5b_mini_pilot \
  --bin download_gaia_xp_continuous_bulk 2>&1 | tail -3

BULK_PIPELINE="$NSB_REPO/target/release/run_starlight_xp_continuous_bulk_pipeline"

retry_delay_sec() {
  local attempt="$1"
  local delay="$BULK_LOOP_RETRY_BASE_SEC"
  local i
  for ((i = 1; i < attempt; i++)); do
    if ((delay >= BULK_LOOP_RETRY_MAX_SEC)); then
      delay="$BULK_LOOP_RETRY_MAX_SEC"
      break
    fi
    delay=$((delay * 2))
  done
  if ((delay > BULK_LOOP_RETRY_MAX_SEC)); then
    delay="$BULK_LOOP_RETRY_MAX_SEC"
  fi
  echo "$delay"
}

run_pipeline_iteration() {
  "$BULK_PIPELINE" \
    --work-dir "$STARLIGHT_WORK" \
    --checkpoint-dir "$STARLIGHT_CHECKPOINTS" \
    --manifest-dir "$GAIA_USB_MANIFESTS" \
    --reconciliation-dir "$GAIA_USB_RECONCILIATION" \
    --skip-rehearsal \
    --skip-resume-test \
    --resume \
    --file-limit "$PRODUCTION_PARALLEL_PARTITIONS" \
    --production-parallel-partitions "$PRODUCTION_PARALLEL_PARTITIONS" \
    --production-row-limit 0 \
    --production-batch-size "$PRODUCTION_BATCH_SIZE" \
    --production-workers "$PRODUCTION_WORKERS" \
    --production-worker-headroom "$PRODUCTION_WORKER_HEADROOM" \
    --production-checkpoint-interval "$PRODUCTION_CHECKPOINT_INTERVAL" \
    --frozen-policy "$STARLIGHT_FROZEN_POLICY" \
    --gaiaxpy-environment "$STARLIGHT_GAIAXPY_ENV" \
    --usb-mountpoint "$GAIA_USB_MOUNT" \
    --usb-cache-root "$GAIA_USB_ROOT" \
    --merge-partition-checkpoints \
    --cleanup-verified-inputs \
    --cleanup-limit "$PRODUCTION_PARALLEL_PARTITIONS"
}

ITER=0
while true; do
  ITER=$((ITER + 1))
  echo "--- iteration $ITER $(date -u -Iseconds) ---"
  ATTEMPT=0
  while true; do
    ATTEMPT=$((ATTEMPT + 1))
    if ((ATTEMPT > 1)); then
      echo "iteration $ITER retry attempt $ATTEMPT"
    fi
    if run_pipeline_iteration; then
      break
    fi
    echo "pipeline iteration $ITER failed (attempt $ATTEMPT)"
    if ((BULK_LOOP_MAX_RETRIES > 0 && ATTEMPT >= BULK_LOOP_MAX_RETRIES)); then
      echo "max retries ($BULK_LOOP_MAX_RETRIES) reached; stopping loop"
      exit 1
    fi
    if ! mountpoint -q "$GAIA_USB_MOUNT"; then
      echo "USB unmounted at $GAIA_USB_MOUNT; stopping loop"
      exit 1
    fi
    DELAY="$(retry_delay_sec "$ATTEMPT")"
    echo "backing off ${DELAY}s before retry (CDN/network blips are expected)"
    sleep "$DELAY"
  done
  echo "iteration $ITER complete"
done
