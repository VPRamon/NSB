#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
# Complete native schedules, scan600, independent fine references and paired timings.
# Optional NSB_DISCOVERY_BINARY selects a preserved release benchmark executable.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-2}"
export NSB_DISCOVERY_REPEATS="${NSB_DISCOVERY_REPEATS:-3}"
output="${1:-target/adaptive-matrix.log}"
mkdir -p "$(dirname "$output")"
if [[ -n "${NSB_DISCOVERY_BINARY:-}" ]]; then
    "$NSB_DISCOVERY_BINARY" complete_adaptive_schedule_matrix --ignored --nocapture --test-threads=1 | tee "$output"
else
    cargo test --release --locked -p nsb --lib complete_adaptive_schedule_matrix -- --ignored --nocapture --test-threads=1 | tee "$output"
fi
