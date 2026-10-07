#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECK="$ROOT/scripts/check-cfg-test-boundaries.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

write_source() {
  rm -rf "$TMP/crates"
  mkdir -p "$TMP/crates/example/src"
  cat >"$TMP/crates/example/src/lib.rs"
}

expect_pass() {
  python3 "$CHECK" --root "$TMP" >/dev/null
}

expect_fail() {
  local output status=0
  output="$(python3 "$CHECK" --root "$TMP" 2>&1)" || status=$?
  [[ "$status" -eq 1 ]] || {
    printf 'expected failure, got status %s\n%s\n' "$status" "$output" >&2
    exit 1
  }
  [[ "$output" == *"gates non-module item"* ]] || {
    printf 'missing diagnostic:\n%s\n' "$output" >&2
    exit 1
  }
}

write_source <<'EOF'
#[cfg(test)]
mod tests {}

#[cfg(test)]
#[path = "tests.rs"]
mod external_tests;

#[cfg(test)]
pub(crate) mod test_support {}
EOF
expect_pass

write_source <<'EOF'
#[cfg(test)]
use crate::fixture;
EOF
expect_fail

write_source <<'EOF'
struct Example {
    #[cfg(test)]
    test_state: bool,
}
EOF
expect_fail

write_source <<'EOF'
impl Example {
    #[cfg(test)]
    fn test_constructor() -> Self {
        Self {}
    }
}
EOF
expect_fail

write_source <<'EOF'
fn production() {
    #[cfg(test)]
    panic!("test-only branch");
}
EOF
expect_fail

echo "cfg(test) boundary checker tests: PASS"
