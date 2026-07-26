#!/usr/bin/env bash
# Package an honest 336–650 nm XP continuous milestone candidate (issue #47 subset).
set -euo pipefail

NSB_REPO="${NSB_REPO:-/path/to/nsb}"
STARLIGHT_ROOT="${STARLIGHT_ROOT:-$HOME/nsb-data/starlight-gaia-release}"
STARLIGHT_CHECKPOINTS="${STARLIGHT_CHECKPOINTS:-$STARLIGHT_ROOT/checkpoints}"
GAIA_USB_ROOT="${GAIA_USB_ROOT:-/path/to/external-storage/nsb-data/gaia-bulk}"
WORK_DIR="${WORK_DIR:-$STARLIGHT_ROOT/work/milestone_pack}"
CANDIDATE_DIR="${CANDIDATE_DIR:-$STARLIGHT_ROOT/candidates/xp_continuous_336_650_week1}"
NSIDE="${NSIDE:-64}"
RELEASE_ID="${RELEASE_ID:-xp_continuous_336_650_week1}"

# The merged accumulator is JSON and is not a runtime HEALPix map. Packaging
# therefore requires the map, diagnostics, and validation artifacts produced by
# the map build/validation stages.
HEALPIX_MAP="${HEALPIX_MAP:-$CANDIDATE_DIR/starlight_mean.release.csv}"
DIAGNOSTICS="${DIAGNOSTICS:-$CANDIDATE_DIR/starlight_source_contributions.diagnostics.json}"
VALIDATION="${VALIDATION:-$CANDIDATE_DIR/starlight.validation.json}"
CONTRIBUTIONS="$CANDIDATE_DIR/xp_continuous_336_650_contributions.csv"
INPUTS_MANIFEST="$CANDIDATE_DIR/xp_continuous_336_650_inputs.toml"
COVERAGE_METADATA="$CANDIDATE_DIR/xp_continuous_336_650_coverage.json"
PACKED_MAP="$CANDIDATE_DIR/xp_continuous_336_650.candidate.csv"
PACKED_MANIFEST="$CANDIDATE_DIR/xp_continuous_336_650.candidate.toml"

mkdir -p "$WORK_DIR" "$CANDIDATE_DIR"
cd "$NSB_REPO"

cargo build --release --locked -p nsb-data-tools \
  --bin run_starlight_xp_continuous_bulk_pipeline \
  --bin export_starlight_healpix_to_contributions \
  --bin nsb-data

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

for required in "$HEALPIX_MAP" "$DIAGNOSTICS" "$VALIDATION"; do
  if [[ ! -f "$required" ]]; then
    echo "required milestone artifact missing: $required" >&2
    exit 1
  fi
done

cargo run --release --locked -p nsb-data-tools \
  --bin export_starlight_healpix_to_contributions -- \
  --input "$HEALPIX_MAP" \
  --nside "$NSIDE" \
  --output-csv "$CONTRIBUTIONS" \
  --output-manifest "$INPUTS_MANIFEST" \
  --coverage-metadata "$COVERAGE_METADATA" \
  --branch xp_continuous \
  --release-id "$RELEASE_ID"

cargo run --release --locked -p nsb-data-tools \
  --bin nsb-data -- starlight release pack-asset \
  --input "$HEALPIX_MAP" \
  --diagnostics "$DIAGNOSTICS" \
  --validation "$VALIDATION" \
  --output "$PACKED_MAP" \
  --manifest "$PACKED_MANIFEST" \
  --candidate

echo "milestone candidate staged under $CANDIDATE_DIR"
