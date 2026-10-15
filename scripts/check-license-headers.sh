#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
# Verify mandatory SPDX/copyright headers on tracked Rust and Python sources.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
failed=0
while IFS= read -r -d '' path; do
  case "$path" in
    *.rs) prefix='//' ;;
    *.py|*.pyi) prefix='#' ;;
    *) continue ;;
  esac

  exec 3< "$path"
  IFS= read -r first <&3 || true
  if [[ "$first" == '#!'* ]]; then
    IFS= read -r first <&3 || true
  fi
  IFS= read -r second <&3 || true
  exec 3<&-

  if [[ "$first" != "$prefix SPDX-License-Identifier: AGPL-3.0-only" ||
        "$second" != "$prefix Copyright (C) 2026 Vallés Puig, Ramon" ]]; then
    if (( failed == 0 )); then
      printf 'Missing or invalid SPDX license header:\n' >&2
    fi
    printf '  %s\n' "$path" >&2
    failed=1
  fi
done < <(git ls-files -z)
exit "$failed"
