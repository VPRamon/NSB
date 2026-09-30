#!/usr/bin/env bash
# Publish the NSB crate when its version is not yet on crates.io.
#
# Usage (from repository root):
#   CARGO_REGISTRY_TOKEN=... bash scripts/publish-changed.sh
#   CARGO_REGISTRY_TOKEN=... bash scripts/publish-changed.sh --dry-run
#
# Tag-triggered releases (for example v0.1.0) should set
# CARGO_REGISTRY_TOKEN in CI.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DRY_RUN=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h | --help)
      sed -n '2,9p' "$0"
      exit 0
      ;;
    *)
      echo "error: unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "error: CARGO_REGISTRY_TOKEN must be set to publish to crates.io" >&2
  exit 1
fi

export CARGO_REGISTRY_TOKEN

MANIFEST="$ROOT/crates/nsb/Cargo.toml"

crate_name() {
  cargo metadata --no-deps --format-version 1 --manifest-path "$1" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["name"])'
}

crate_version() {
  cargo metadata --no-deps --format-version 1 --manifest-path "$1" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])'
}

already_on_crates_io() {
  local name="$1"
  local version="$2"

  if cargo search "$name" --limit 1 2>/dev/null | grep -q "^$name = \"$version\""; then
    return 0
  fi
  return 1
}

name="$(crate_name "$MANIFEST")"
version="$(crate_version "$MANIFEST")"

if [[ -n "${GITHUB_REF_NAME:-}" ]]; then
  expected_tag="v$version"
  if [[ "$GITHUB_REF_NAME" != "$expected_tag" ]]; then
    echo "error: tag $GITHUB_REF_NAME does not match $name version $version ($expected_tag)" >&2
    exit 1
  fi
fi

if already_on_crates_io "$name" "$version"; then
  echo "skip $name $version: already on crates.io"
  exit 0
fi

echo "publish $name $version from $MANIFEST"
if [[ "$DRY_RUN" -eq 1 ]]; then
  cargo publish --manifest-path "$MANIFEST" --dry-run --allow-dirty
else
  cargo publish --manifest-path "$MANIFEST" --allow-dirty
fi

echo "publish-changed: done"
