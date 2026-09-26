#!/usr/bin/env bash
# Direct cargo-public-api checks for the `nsb` crate (#176).
#
# Lifecycle:
#   pre-freeze  — forbidden-API grep only (no snapshot / SemVer)
#   freeze bootstrap — snapshot must match HEAD; historical diff skipped
#   post-freeze — snapshot match + cargo-public-api diff BASE..HEAD
#
# Usage:
#   scripts/check-public-api.sh              # check (requires NSB_PUBLIC_API_BASE when frozen)
#   scripts/check-public-api.sh --write      # regenerate crates/nsb/api/public-api.txt
#   scripts/check-public-api.sh --base REV   # historical base (PR/push SHA)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NIGHTLY="${NSB_PUBLIC_API_RUSTDOC_TOOLCHAIN:-nightly-2026-09-02}"
TOOL_VERSION="${NSB_PUBLIC_API_TOOL_VERSION:-0.50.1}"
SNAPSHOT="crates/nsb/api/public-api.txt"
MARKER="crates/nsb/api/API_FROZEN"
GENERATED="$(mktemp)"
trap 'rm -f "$GENERATED"' EXIT

WRITE=0
BASE="${NSB_PUBLIC_API_BASE:-}"
BASE_EXPLICIT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --write) WRITE=1; shift ;;
    --base)
      BASE="${2:?--base requires a revision}"
      BASE_EXPLICIT=1
      shift 2
      ;;
    -h|--help)
      sed -n '1,20p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

ensure_tool() {
  local installed
  installed="$(cargo public-api --version 2>/dev/null | awk '{print $2}' || true)"
  if [[ "$installed" != "$TOOL_VERSION" ]]; then
    cargo install cargo-public-api --locked --version "$TOOL_VERSION" --force
  fi
}

generate() {
  cargo "+${NIGHTLY}" public-api \
    -p nsb \
    -sss \
    --all-features \
    --color=never
}

# Narrow guards against deliberately removed public debt (#175 / #176).
reject_forbidden_public_api() {
  local api_file="$1"
  local pattern
  pattern='WindowSearchDiagnostics|periods_below_threshold_diagnosed|prepare_and_search_diagnosed|StarlightProvenance::test_fixture|StarlightMap::pixel_lon_lat_deg|MagnitudesPerAirmass|SolarSpectralIrradiance|pub struct Starlight \{|pub struct StarlightOutputs \{|pub struct ZodiacalLight|pub struct ZodiacalOutputs|SiteCalibrationAsset|CalibratedSiteId|AirglowCalibrationEvidence|pub mod atmosphere|nsb::error::'
  if grep -E "$pattern" "$api_file" >/dev/null; then
    echo "forbidden public API detected:" >&2
    grep -nE "$pattern" "$api_file" >&2 || true
    exit 1
  fi
}

ensure_tool
generate >"$GENERATED"
reject_forbidden_public_api "$GENERATED"

if [[ "$WRITE" -eq 1 ]]; then
  mkdir -p "$(dirname "$SNAPSHOT")"
  cp "$GENERATED" "$SNAPSHOT"
  echo "wrote $SNAPSHOT"
  exit 0
fi

if [[ ! -f "$MARKER" ]]; then
  echo "SemVer gate: public API is pre-freeze; snapshot and historical checks skipped"
  echo "public API policy: pre-freeze (forbidden-API guard only)"
  exit 0
fi

if [[ ! -f "$SNAPSHOT" ]]; then
  echo "freeze marker exists but $SNAPSHOT is missing" >&2
  exit 1
fi

if ! diff -u "$SNAPSHOT" "$GENERATED"; then
  echo "public API snapshot drift: run scripts/check-public-api.sh --write" >&2
  exit 1
fi

if [[ -z "$BASE" ]]; then
  if [[ "$BASE_EXPLICIT" -eq 1 ]]; then
    echo "explicit empty --base is invalid" >&2
    exit 1
  fi
  echo "SemVer gate: no historical base; bootstrap/snapshot-only mode"
  exit 0
fi

HEAD_SHA="$(git rev-parse HEAD)"
BASE_SHA="$(git rev-parse --verify "$BASE^{commit}")"

if [[ "$BASE_SHA" == "$HEAD_SHA" ]]; then
  echo "invalid historical comparison: BASE == HEAD ($BASE_SHA)" >&2
  exit 1
fi

if ! git cat-file -e "${BASE_SHA}:${MARKER}" 2>/dev/null; then
  echo "SemVer gate: base $BASE_SHA is not frozen; bootstrap/snapshot-only mode"
  exit 0
fi

if ! git cat-file -e "${BASE_SHA}:${SNAPSHOT}" 2>/dev/null; then
  echo "SemVer gate: base $BASE_SHA lacks $SNAPSHOT; bootstrap/snapshot-only mode"
  exit 0
fi

echo "SemVer gate: cargo public-api diff ${BASE_SHA}..HEAD --deny=removed --deny=changed"
cargo "+${NIGHTLY}" public-api diff \
  "${BASE_SHA}..HEAD" \
  -p nsb \
  -sss \
  --all-features \
  --deny=removed \
  --deny=changed \
  --color=never

echo "public API policy: frozen (snapshot + historical SemVer)"
