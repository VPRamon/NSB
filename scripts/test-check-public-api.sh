#!/usr/bin/env bash
# Focused lifecycle tests for scripts/check-public-api.sh (#176).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
SCRIPT="$ROOT/scripts/check-public-api.sh"
MARKER="crates/nsb/api/API_FROZEN"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

pass() {
  echo "PASS: $*"
}

# Snapshot equality is assumed for these control-flow tests; regenerate once.
"$SCRIPT" --write >/dev/null

# Frozen HEAD without base must fail closed.
if "$SCRIPT" >/dev/null 2>&1; then
  fail "frozen HEAD without --base should fail"
fi
pass "frozen + missing base fails"

# Explicit empty base fails.
if "$SCRIPT" --base "" >/dev/null 2>&1; then
  fail "explicit empty --base should fail"
fi
pass "explicit empty base fails"

# BASE == HEAD fails.
if "$SCRIPT" --base HEAD >/dev/null 2>&1; then
  fail "BASE == HEAD should fail"
fi
pass "BASE == HEAD fails"

# Unresolvable base fails.
if "$SCRIPT" --base definitely-not-a-git-rev-zzzz >/dev/null 2>&1; then
  fail "unresolvable base should fail"
fi
pass "unresolvable base fails"

# Bootstrap against a non-frozen ancestor (main tip before this freeze) succeeds snapshot-only.
NON_FROZEN_BASE="$(git rev-list --max-count=1 HEAD --not --grep='.' 2>/dev/null || true)"
# Prefer origin/main if it lacks the freeze marker.
if git rev-parse --verify origin/main >/dev/null 2>&1; then
  MAIN_SHA="$(git rev-parse origin/main)"
  if ! git cat-file -e "${MAIN_SHA}:${MARKER}" 2>/dev/null; then
    out="$("$SCRIPT" --base "$MAIN_SHA" 2>&1 || true)"
    if [[ "$out" != *"freeze-bootstrap"* ]]; then
      fail "expected freeze-bootstrap against non-frozen main; got: $out"
    fi
    pass "non-frozen historical base bootstraps snapshot-only"
  else
    pass "skip bootstrap case (origin/main already frozen)"
  fi
else
  pass "skip bootstrap case (no origin/main)"
fi

echo "all check-public-api lifecycle assertions passed"
