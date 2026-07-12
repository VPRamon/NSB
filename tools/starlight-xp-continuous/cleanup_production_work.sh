#!/usr/bin/env bash
# Remove production_loop temporaries for partitions with HEALPix checkpoints.
set -euo pipefail

STARLIGHT_ROOT="${STARLIGHT_ROOT:-$HOME/nsb-data/starlight-gaia-release}"
WORK_ROOT="$STARLIGHT_ROOT/work/production_loop"
CHECKPOINT_DIR="$STARLIGHT_ROOT/checkpoints"

if [[ ! -d "$WORK_ROOT" ]]; then
  echo "no work root: $WORK_ROOT"
  exit 0
fi

freed=0
for dir in "$WORK_ROOT"/*; do
  [[ -d "$dir" ]] || continue
  stem="$(basename "$dir")"
  checkpoint="$CHECKPOINT_DIR/${stem}_healpix_accumulator.json"
  if [[ -f "$checkpoint" ]]; then
    bytes=$(du -sb "$dir" | awk '{print $1}')
    echo "removing completed work: $stem ($bytes bytes)"
    rm -rf "$dir"
    freed=$((freed + bytes))
  else
    echo "keeping in-progress work (no checkpoint): $stem"
  fi
done

echo "freed_bytes=$freed"
