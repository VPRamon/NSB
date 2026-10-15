#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
# Reject test-only implementation hooks in Rust production source trees.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${1:-}" == --root && $# -eq 2 ]]; then
  ROOT="$2"
elif (( $# != 0 )); then
  printf 'usage: %s [--root PATH]\n' "$0" >&2
  exit 2
fi

shopt -s globstar nullglob
status=0
for file in "$ROOT"/crates/*/src/**/*.rs; do
  [[ -f "$file" ]] || continue
  awk '
    function brackets(s, left, right) {
      left = s; right = s
      return gsub(/\[/, "", left) - gsub(/\]/, "", right)
    }
    function violation(reason, line) {
      printf "%s:%d: cfg(test) gates non-module item at line %d: %s\n", FILENAME, start, line, reason > "/dev/stderr"
      failed = 1
    }
    {
      if (pending) {
        if (attribute_depth > 0) {
          attribute_depth += brackets($0)
          next
        }
        if ($0 ~ /^[[:space:]]*$/) next
        if ($0 ~ /^[[:space:]]*#[[:space:]]*\[/) {
          attribute_depth = brackets($0)
          next
        }
        if ($0 !~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[a-zA-Z_][a-zA-Z_0-9]*[[:space:]]*(;|\{)/) {
          violation($0, NR)
        }
        pending = 0
      }
      if ($0 ~ /^[[:space:]]*#[[:space:]]*\[[[:space:]]*cfg[[:space:]]*\([[:space:]]*test[[:space:]]*\)[[:space:]]*\][[:space:]]*$/) {
        pending = 1
        start = NR
      }
    }
    END {
      if (pending) violation("<end of file>", NR)
      exit failed
    }
  ' "$file" || status=1
done

if (( status )); then
  echo 'cfg(test) boundary check: FAIL' >&2
else
  echo 'cfg(test) boundary check: PASS'
fi
exit "$status"
