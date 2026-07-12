#!/usr/bin/env bash
# Orchestrate Phase 5 XP continuous acquisition, reconstruction, validation, and contributions.
set -euo pipefail

ROOT="${NSB_ROOT:-/home/valles/workspace/nsb}"
PHASE5="${PHASE5_ROOT:-$HOME/nsb-data/starlight-gaia-release/missing-flux/phase5}"
VENV="${ROOT}/tools/starlight-xp-continuous/.venv/bin/python"
MISSING_FLUX="${MISSING_FLUX_ROOT:-$HOME/nsb-data/starlight-gaia-release/missing-flux}"
CATALOGUE="${CATALOGUE:-$HOME/nsb-data/starlight-gaia-release/gaia_dr3_starlight_sources.csv}"

cd "$ROOT"

echo "== Phase 5 prepare =="
cargo run --locked -q -p nsb-data-tools --bin prepare_starlight_phase5 -- \
  --missing-flux-root "$MISSING_FLUX" \
  --phase5-root "$PHASE5"

echo "== GaiaXPy environment audit =="
"$VENV" "$ROOT/tools/starlight-xp-continuous/audit_gaiaxpy_environment.py" \
  --output-json "$PHASE5/phase5_gaiaxpy_environment.json" \
  --output-sha256 "$PHASE5/phase5_gaiaxpy_environment.sha256"

echo "== XP continuous download (resume) =="
mkdir -p "$PHASE5/coefficients/raw"
cargo run --locked -q -p nsb-data-tools --bin download_xp_continuous_phase5 -- \
  --targets-csv "$PHASE5/phase5_all_xp_continuous_targets.csv" \
  --raw-dir "$PHASE5/coefficients/raw" \
  --checkpoint "$PHASE5/coefficients/checkpoint.jsonl" \
  --inventory-csv "$PHASE5/phase5_download_inventory.csv" \
  --manifest-json "$PHASE5/phase5_requests.manifest.json" \
  --resume

echo "== Download inventory inspect =="
cargo run --locked -q -p nsb-data-tools --bin inspect_phase5_download -- \
  --phase5-root "$PHASE5" \
  --targets-csv "$PHASE5/phase5_all_xp_continuous_targets.csv"

echo "== Normalize coefficients =="
mkdir -p "$PHASE5/coefficients/canonical"
cargo run --locked -q -p nsb-data-tools --bin normalize_xp_continuous_coefficients -- \
  --raw-dir "$PHASE5/coefficients/raw" \
  --output-dir "$PHASE5/coefficients/canonical" \
  --manifest-json "$PHASE5/phase5_coefficients.manifest.json"

echo "== Rust in-process reconstruction =="
mkdir -p "$PHASE5/reconstruction/normalized"
cargo run --release --locked -q -p nsb-data-tools --bin reconstruct_canonical_coefficients -- \
  --coefficients-dir "$PHASE5/coefficients/canonical" \
  --output-dir "$PHASE5/reconstruction/normalized" \
  --manifest "$PHASE5/phase5_reconstruction.manifest.json" \
  --gaiaxpy-environment "$PHASE5/phase5_gaiaxpy_environment.json"

CAL_SHA="$(grep -o '^[0-9a-f]\{64\}' "$PHASE5/phase5_gaiaxpy_environment.sha256" | head -1)"

echo "== Overlap validation =="
cargo run --locked -q -p nsb-data-tools --bin run_starlight_phase5_overlap_validation -- \
  --overlap-targets "$PHASE5/phase5_overlap_targets.csv" \
  --reconstructed-dir "$PHASE5/reconstruction/normalized" \
  --canonical-catalogue "$CATALOGUE" \
  --exclusions-csv "$HOME/nsb-data/starlight-gaia-release/gaia_dr3_starlight_exclusions.csv" \
  --output-json "$PHASE5/phase5_overlap_validation.json" \
  --output-md "$PHASE5/phase5_overlap_validation.md" \
  --predictions-csv "$PHASE5/phase5_overlap_predictions.csv" \
  --stratified-csv "$PHASE5/phase5_overlap_stratified_metrics.csv" \
  --frozen-policy-json "$PHASE5/phase5_frozen_validation_policy.json" \
  --phase5-exclusions-csv "$PHASE5/phase5_exclusions.csv" \
  --phase5-root "$PHASE5"

echo "== Continuous-only contributions =="
mkdir -p "$PHASE5/reconstruction/normalized"
CAL_SHA="$(grep -o '^[0-9a-f]\{64\}' "$PHASE5/phase5_gaiaxpy_environment.sha256" | head -1)"
cargo run --locked -q -p nsb-data-tools --bin emit_phase5_continuous_contributions -- \
  --continuous-only-targets "$PHASE5/phase5_continuous_only_targets.csv" \
  --reconstructed-dir "$PHASE5/reconstruction/normalized" \
  --output-csv "$PHASE5/phase5_continuous_only_336_650.csv" \
  --reconciliation-json "$PHASE5/phase5_population_reconciliation.partial.json" \
  --calibration-checksum "$CAL_SHA"

echo "== Finalize reconciliation and checksums =="
cargo run --locked -q -p nsb-data-tools --bin finalize_starlight_phase5 -- \
  --phase5-root "$PHASE5" \
  --overlap-targets "$PHASE5/phase5_overlap_targets.csv" \
  --continuous-only-targets "$PHASE5/phase5_continuous_only_targets.csv" \
  --raw-dir "$PHASE5/coefficients/raw" \
  --reconstructed-dir "$PHASE5/reconstruction/normalized" \
  --overlap-validation-json "$PHASE5/phase5_overlap_validation.json" \
  --output-reconciliation "$PHASE5/phase5_population_reconciliation.json" \
  --exclusions-csv "$PHASE5/phase5_exclusions.csv"

echo "Phase 5 pipeline complete -> $PHASE5"
