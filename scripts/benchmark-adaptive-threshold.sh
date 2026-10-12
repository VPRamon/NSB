#!/usr/bin/env bash
# Internal #234 harness; no public diagnostics API or Cargo feature.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-2}"
export NSB_DISCOVERY_REPEATS="${NSB_DISCOVERY_REPEATS:-3}"
cargo test --release --locked -p nsb --lib complete_adaptive_schedule_matrix -- --ignored --nocapture --test-threads=1
