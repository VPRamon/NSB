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

if [[ "$DRY_RUN" -eq 0 && -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "error: CARGO_REGISTRY_TOKEN must be set to publish to crates.io" >&2
  exit 1
fi

export CARGO_REGISTRY_TOKEN

MANIFEST="$ROOT/crates/nsb/Cargo.toml"

# Cargo already knows the package identity; no embedded Python is required.
name="nsb"
pkg_id="$(cargo pkgid --locked --manifest-path "$MANIFEST" -p "$name")"
version="${pkg_id##*#}"
version="${version##*@}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.+-]+)?$ ]]; then
  echo "error: unable to resolve $name version from cargo pkgid: $pkg_id" >&2
  exit 1
fi

already_on_crates_io() {
  local name="$1"
  local version="$2"
  local url="https://crates.io/api/v1/crates/$name/$version"
  local status

  if ! status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
    --user-agent 'nsb-release-workflow (https://github.com/VPRamon/NSB)' \
    "$url")"; then
    echo "error: failed to query crates.io for $name $version" >&2
    return 2
  fi

  case "$status" in
    200) return 0 ;;
    404) return 1 ;;
    *)
      echo "error: crates.io returned HTTP $status for $name $version" >&2
      return 2
      ;;
  esac
}

publication_status="$(git status --porcelain --untracked-files=normal)"
if [[ -n "$publication_status" ]]; then
  echo "error: publication requires a clean committed checkout" >&2
  exit 1
fi


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
else
  status=$?
  if [[ "$status" -ne 1 ]]; then
    exit "$status"
  fi
fi

echo "publish $name $version from $MANIFEST"
if [[ "$DRY_RUN" -eq 1 ]]; then
  cargo publish --manifest-path "$MANIFEST" --dry-run --locked
else
  cargo publish --manifest-path "$MANIFEST" --locked
fi

echo "publish-changed: done"
