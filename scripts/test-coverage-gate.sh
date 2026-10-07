#!/usr/bin/env bash
# Focused regression tests for scripts/coverage-gate.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
SCRIPT="$ROOT/scripts/coverage-gate.sh"
FIX="$ROOT/scripts/fixtures/coverage-gate"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

pass() {
  echo "PASS: $*"
}

assert_contains() {
  local haystack="$1" needle="$2" label="$3"
  [[ "$haystack" == *"$needle"* ]] || fail "$label: expected to contain: $needle\n---\n$haystack"
}

assert_exit() {
  local expected="$1"
  shift
  local status=0
  "$@" >"$TMP/out" 2>"$TMP/err" || status=$?
  [[ "$status" -eq "$expected" ]] || fail "exit $expected expected, got $status\nstdout:\n$(cat "$TMP/out")\nstderr:\n$(cat "$TMP/err")"
}

# ---------------------------------------------------------------------------
# overall
# ---------------------------------------------------------------------------

assert_exit 0 "$SCRIPT" overall --policy "$FIX/policy.toml" --lcov "$FIX/overall-pass.lcov"
assert_contains "$(cat "$TMP/out")" "result: PASS" "overall pass"
pass "overall coverage passes above floors"

assert_exit 1 "$SCRIPT" overall --policy "$FIX/policy.toml" --lcov "$FIX/overall-workspace-fail.lcov"
assert_contains "$(cat "$TMP/out")" "workspace line coverage" "workspace fail message"
assert_contains "$(cat "$TMP/out")" "result: FAIL" "workspace fail result"
pass "overall workspace floor failure"

assert_exit 1 "$SCRIPT" overall --policy "$FIX/policy.toml" --lcov "$FIX/overall-nsb-fail.lcov"
assert_contains "$(cat "$TMP/out")" "nsb line coverage" "nsb fail message"
assert_contains "$(cat "$TMP/out")" "result: FAIL" "nsb fail result"
pass "nsb floor failure"

assert_exit 1 "$SCRIPT" overall --policy "$FIX/policy.toml" --lcov "$FIX/overall-missing-nsb.lcov"
assert_contains "$(cat "$TMP/out")" "no nsb coverage data" "missing nsb message"
assert_contains "$(cat "$TMP/out")" "result: FAIL" "missing nsb result"
pass "missing nsb LCOV data fails closed"

# ---------------------------------------------------------------------------
# diff
# ---------------------------------------------------------------------------

assert_exit 0 "$SCRIPT" diff \
  --policy "$FIX/policy.toml" \
  --lcov "$FIX/diff-covered.lcov" \
  --diff-file "$FIX/diff-covered.diff"
assert_contains "$(cat "$TMP/out")" "result: PASS" "diff covered pass"
pass "covered changed production line passes"

assert_exit 1 "$SCRIPT" diff \
  --policy "$FIX/policy.toml" \
  --lcov "$FIX/diff-uncovered.lcov" \
  --diff-file "$FIX/diff-uncovered.diff"
assert_contains "$(cat "$TMP/out")" "uncovered changed production lines:" "uncovered list"
assert_contains "$(cat "$TMP/out")" "crates/nsb/src/lib.rs:10" "uncovered location"
assert_contains "$(cat "$TMP/out")" "result: FAIL" "uncovered fail"
pass "uncovered changed production line fails"

assert_exit 0 "$SCRIPT" diff \
  --policy "$FIX/policy.toml" \
  --lcov "$FIX/diff-test-only.lcov" \
  --diff-file "$FIX/diff-test-only.diff"
assert_contains "$(cat "$TMP/out")" "no executable changed production lines" "test-only message"
assert_contains "$(cat "$TMP/out")" "result: PASS" "test-only pass"
pass "changed test-only Rust source does not count"

cat >"$TMP/python-boundary.diff" <<'EOF'
diff --git a/crates/nsb/src/python/api.rs b/crates/nsb/src/python/api.rs
--- a/crates/nsb/src/python/api.rs
+++ b/crates/nsb/src/python/api.rs
@@ -1,0 +2,1 @@
+pub fn wheel_tested_adapter() {}
EOF
assert_exit 0 "$SCRIPT" diff \
  --policy "$FIX/policy.toml" \
  --lcov "$FIX/diff-covered.lcov" \
  --diff-file "$TMP/python-boundary.diff"
assert_contains "$(cat "$TMP/out")" "no executable changed production lines" "Python boundary message"
assert_contains "$(cat "$TMP/out")" "result: PASS" "Python boundary pass"
pass "installed-wheel-tested Python adapter does not count as Rust diff coverage"

cat >"$TMP/moonlight-generator.diff" <<'EOF'
diff --git a/crates/nsb-data-tools/src/dataset/moonlight_multiscatter.rs b/crates/nsb-data-tools/src/dataset/moonlight_multiscatter.rs
--- a/crates/nsb-data-tools/src/dataset/moonlight_multiscatter.rs
+++ b/crates/nsb-data-tools/src/dataset/moonlight_multiscatter.rs
@@ -1,0 +2,1 @@
+pub fn offline_scientific_generator() { expensive_science(); }
EOF
assert_exit 0 "$SCRIPT" diff \
  --policy "$FIX/policy.toml" \
  --lcov "$FIX/diff-covered.lcov" \
  --diff-file "$TMP/moonlight-generator.diff"
assert_contains "$(cat "$TMP/out")" "no executable changed production lines" "moonlight generator message"
assert_contains "$(cat "$TMP/out")" "result: PASS" "moonlight generator pass"
pass "offline moonlight generator does not count as PR diff production coverage"

assert_exit 0 "$SCRIPT" diff \
  --policy "$FIX/policy.toml" \
  --lcov "$FIX/diff-nonexec.lcov" \
  --diff-file "$FIX/diff-nonexec.diff"
assert_contains "$(cat "$TMP/out")" "0/0 executable" "nonexec counts"
assert_contains "$(cat "$TMP/out")" "result: PASS" "nonexec pass"
pass "non-executable changed lines do not reduce coverage"

assert_exit 1 "$SCRIPT" diff \
  --policy "$FIX/policy.toml" \
  --lcov "$FIX/diff-missing-file.lcov" \
  --diff-file "$FIX/diff-missing-file.diff"
assert_contains "$(cat "$TMP/out")" "no coverage information" "missing file message"
assert_contains "$(cat "$TMP/out")" "result: FAIL" "missing file fail"
pass "production file with unexpectedly missing coverage fails closed"

# Covered test lines must not hide uncovered production lines in the same diff.
# Build a temporary source tree so #[cfg(test)] scanning sees real files.
mkdir -p "$TMP/tree/crates/nsb/src"
cat >"$TMP/tree/crates/nsb/src/mixed.rs" <<'EOF'
pub fn production() {
    let x = 1;
}

#[cfg(test)]
mod tests {
    #[test]
    fn covered_test() {
        assert_eq!(1, 1);
    }
}
EOF
cp "$FIX/policy.toml" "$TMP/tree/coverage-policy.toml"
cat >"$TMP/tree/mixed.lcov" <<'EOF'
SF:crates/nsb/src/mixed.rs
DA:2,0
DA:8,5
end_of_record
EOF
cat >"$TMP/tree/mixed.diff" <<'EOF'
diff --git a/crates/nsb/src/mixed.rs b/crates/nsb/src/mixed.rs
--- a/crates/nsb/src/mixed.rs
+++ b/crates/nsb/src/mixed.rs
@@ -1,0 +2,1 @@
+    let x = 1;
@@ -7,0 +8,1 @@
+        assert_eq!(1, 1);
EOF

assert_exit 1 "$SCRIPT" diff \
  --policy "$TMP/tree/coverage-policy.toml" \
  --lcov "$TMP/tree/mixed.lcov" \
  --diff-file "$TMP/tree/mixed.diff" \
  --source-root "$TMP/tree"
assert_contains "$(cat "$TMP/out")" "crates/nsb/src/mixed.rs:2" "mixed uncovered production"
assert_contains "$(cat "$TMP/out")" "ignored inline #[cfg(test)] lines: 1" "mixed ignored test"
assert_contains "$(cat "$TMP/out")" "result: FAIL" "mixed fail"
pass "covered test lines cannot hide uncovered production lines"

echo "all coverage-gate assertions passed"
