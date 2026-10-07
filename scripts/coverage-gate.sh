#!/usr/bin/env bash
# Local coverage floors and changed-production-line gates for NSB CI.
#
# Line coverage uses LLVM LCOV DA:<line>,<hits> semantics:
#   hits > 0  → covered
#   hits == 0 → uncovered
#   no DA     → not an instrumented/executable line
#
# Thresholds come only from coverage-policy.toml (never duplicated here).
#
# Diff classification (intentionally simple vs the former Rust crate):
#   - production targets: crates/{nsb,nsb-cli,nsb-data-tools}/src/**.rs
#     excluding */tests.rs, */tests/**, the separately wheel-tested PyO3 adapter,
#     and the offline moonlight Monte Carlo generator (validated by its focused
#     scientific tests rather than PR diff coverage)
#   - ignore top-level #[cfg(test)] items (brace-matched modules/items)
#   - missing LCOV for a changed production file fails closed unless every
#     changed production line is an obvious non-instrumentable declaration
#     (comments, attributes, mod/use/type/struct/enum/const/static, braces).
#   - ambiguous continuation lines without LCOV fail closed (no multi-line
#     declaration context scan).
#
# Usage:
#   scripts/coverage-gate.sh overall --lcov coverage.lcov
#   scripts/coverage-gate.sh diff --lcov coverage.lcov --base origin/main
#   scripts/coverage-gate.sh diff --lcov coverage.lcov --diff-file PATH

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

COMMAND=""
POLICY="coverage-policy.toml"
LCOV=""
BASE=""
DIFF_FILE=""
ARTIFACT_HINT=""
SOURCE_ROOT="."

usage() {
  sed -n '1,28p' "$0"
}

die() {
  echo "error: $*" >&2
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    overall|diff)
      [[ -z "$COMMAND" ]] || die "multiple commands"
      COMMAND="$1"
      shift
      ;;
    --policy)
      POLICY="${2:?--policy requires a path}"
      shift 2
      ;;
    --lcov)
      LCOV="${2:?--lcov requires a path}"
      shift 2
      ;;
    --base)
      BASE="${2:?--base requires a git ref}"
      shift 2
      ;;
    --diff-file)
      DIFF_FILE="${2:?--diff-file requires a path}"
      shift 2
      ;;
    --artifact-hint)
      ARTIFACT_HINT="${2:?--artifact-hint requires text}"
      shift 2
      ;;
    --source-root)
      SOURCE_ROOT="${2:?--source-root requires a path}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$COMMAND" ]] || die "expected overall or diff"
[[ -n "$LCOV" ]] || die "--lcov is required"
[[ -f "$LCOV" ]] || die "LCOV file not found: $LCOV"
[[ -f "$POLICY" ]] || die "policy file not found: $POLICY"
SOURCE_ROOT="$(cd "$SOURCE_ROOT" && pwd)" || die "source root not found: $SOURCE_ROOT"

# ---------------------------------------------------------------------------
# Policy: read a few scalar keys from coverage-policy.toml.
# Supports `key = value` under [section] or dotted keys; rejects nonempty
# exclusions.files arrays.
# ---------------------------------------------------------------------------

policy_get() {
  local key="$1"
  local section="" leaf="" value
  if [[ "$key" == *.* ]]; then
    section="${key%%.*}"
    leaf="${key#*.}"
  else
    leaf="$key"
  fi

  value="$(
    awk -v want_section="$section" -v want_key="$leaf" '
      BEGIN { section = "" }
      /^[ \t]*#/ { next }
      /^[ \t]*$/ { next }
      /^\[/ {
        line = $0
        sub(/^\[/, "", line)
        sub(/\].*$/, "", line)
        gsub(/[ \t]/, "", line)
        section = line
        next
      }
      {
        line = $0
        sub(/[ \t]*#.*/, "", line)
        eq = index(line, "=")
        if (eq == 0) next
        k = substr(line, 1, eq - 1)
        v = substr(line, eq + 1)
        gsub(/^[ \t]+|[ \t]+$/, "", k)
        gsub(/^[ \t]+|[ \t]+$/, "", v)
        if ((want_section == "" && section == "" && k == want_key) ||
            (section == want_section && k == want_key)) {
          print v
          exit
        }
      }
    ' "$POLICY"
  )"
  [[ -n "$value" ]] || die "missing policy key: $key"
  # Strip surrounding quotes from TOML strings.
  if [[ "$value" == \"*\" ]]; then
    value="${value:1:${#value}-2}"
  fi
  printf '%s' "$value"
}

validate_percent() {
  local name="$1" value="$2"
  awk -v n="$name" -v v="$value" 'BEGIN {
    if (v !~ /^-?[0-9]+([.][0-9]+)?$/) {
      printf "error: %s is not a number: %s\n", n, v > "/dev/stderr"
      exit 1
    }
    if (v + 0 < 0 || v + 0 > 100) {
      printf "error: %s must be in [0, 100], got %s\n", n, v > "/dev/stderr"
      exit 1
    }
  }'
}

SCHEMA="$(policy_get schema_version)"
[[ "$SCHEMA" == "1" ]] || die "unsupported coverage policy schema: $SCHEMA"

# Nonempty exclusions.files is unsupported (same contract as the old crate).
if awk '
  BEGIN { in_ex = 0 }
  /^\[exclusions\]/ { in_ex = 1; next }
  /^\[/ { in_ex = 0 }
  in_ex && /^[[:space:]]*files[[:space:]]*=/ {
    line = $0
    sub(/^[^=]*=[[:space:]]*/, "", line)
    gsub(/[[:space:]]/, "", line)
    if (line != "[]") exit 1
  }
' "$POLICY"; then
  :
else
  die "exclusions.files is not empty; nonempty exclusion lists are not supported"
fi

BASELINE_KIND="$(policy_get baseline_kind)"
BASELINE_COMMIT="$(policy_get baseline.commit)"
BASELINE_DATE="$(policy_get baseline.date)"
HTML_ARTIFACT="$(policy_get html_artifact_name)"
JSON_ARTIFACT="$(policy_get json_artifact_name)"
LCOV_ARTIFACT="$(policy_get lcov_artifact_name)"
MEASURED_WS="$(policy_get measured.workspace_lines)"
MEASURED_NSB="$(policy_get measured.nsb_lines)"
FLOOR_WS="$(policy_get floors.workspace_lines)"
FLOOR_NSB="$(policy_get floors.nsb_lines)"
FLOOR_DIFF="$(policy_get diff.changed_production_lines)"
DEFAULT_BASE="$(policy_get diff.base_ref)"

validate_percent floors.workspace_lines "$FLOOR_WS"
validate_percent floors.nsb_lines "$FLOOR_NSB"
validate_percent diff.changed_production_lines "$FLOOR_DIFF"

# ---------------------------------------------------------------------------
# LCOV helpers
# ---------------------------------------------------------------------------

# Emit lines: path<TAB>line<TAB>hits  (repo-relative path, max hits per line)
normalize_lcov() {
  awk '
    function repo_relative(path,   i) {
      gsub(/\\/, "/", path)
      i = index(path, "/crates/")
      if (i > 0) return substr(path, i + 1)
      if (substr(path, 1, 2) == "./") return substr(path, 3)
      if (substr(path, 1, 7) == "crates/") return path
      n = split(path, parts, "/")
      return parts[n]
    }
    /^[[:space:]]*#/ { next }
    /^TN:/ { next }
    /^SF:/ {
      path = substr($0, 4)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", path)
      path = repo_relative(path)
      next
    }
    /^DA:/ {
      rest = substr($0, 4)
      split(rest, a, ",")
      line = a[1] + 0
      hits = a[2] + 0
      key = path SUBSEP line
      if (!(key in max) || hits > max[key]) max[key] = hits
      next
    }
    END {
      for (key in max) {
        split(key, p, SUBSEP)
        printf "%s\t%d\t%d\n", p[1], p[2], max[key]
      }
    }
  ' "$LCOV" | LC_ALL=C sort
}

LCOV_TABLE="$(mktemp)"
trap 'rm -f "$LCOV_TABLE" ${CHANGED_TMP:-} ${CFG_TMP:-} ${PROD_TMP:-}' EXIT
normalize_lcov >"$LCOV_TABLE"

crate_line_stats() {
  # Args: crate name (exact path prefix crates/<name>/)
  # Prints: covered count percent  (or "missing" if absent)
  local crate="$1"
  awk -v crate="$crate" -F '\t' '
    BEGIN { covered = 0; count = 0 }
    index($1, "crates/" crate "/") == 1 {
      count++
      if ($3 + 0 > 0) covered++
    }
    END {
      if (count == 0) {
        print "missing"
        exit
      }
      printf "%d %d %.10f\n", covered, count, (covered * 100.0) / count
    }
  ' "$LCOV_TABLE"
}

workspace_line_stats() {
  awk -F '\t' '
    BEGIN { covered = 0; count = 0 }
    {
      count++
      if ($3 + 0 > 0) covered++
    }
    END {
      if (count == 0) {
        print "0 0 0"
        exit
      }
      printf "%d %d %.10f\n", covered, count, (covered * 100.0) / count
    }
  ' "$LCOV_TABLE"
}

is_production_rust_file() {
  local path="$1"
  case "$path" in
    crates/nsb/src/*|crates/nsb-cli/src/*|crates/nsb-data-tools/src/*) ;;
    *) return 1 ;;
  esac
  [[ "$path" == *.rs ]] || return 1
  case "$path" in
    */tests.rs|*/tests/*|crates/nsb/src/python/*) return 1 ;;
    crates/nsb-data-tools/src/dataset/moonlight_multiscatter.rs) return 1 ;;
  esac
  return 0
}

# True when a single changed line is an obvious non-instrumentable declaration.
# Conservative: ambiguous text returns false (fail closed when LCOV is missing).
is_non_instrumentable_line() {
  local text="$1"
  local t
  t="$(printf '%s' "$text" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
  [[ -z "$t" ]] && return 0
  [[ "$t" == //* ]] && return 0
  [[ "$t" == \#\[* || "$t" == \#!\[* ]] && return 0
  [[ "$t" == /\** || "$t" == "*/" ]] && return 0
  # Block-comment / doc continuations (` * ...`), not raw pointers.
  if [[ "$t" == \** && "$t" != \*mut* && "$t" != \*const* ]]; then
    return 0
  fi
  case "$t" in
    '{'|'}'|'};'|');'|','|';') return 0 ;;
  esac

  # Strip common visibility prefixes.
  local body="$t"
  body="${body#pub(crate) }"
  body="${body#pub(super) }"
  body="${body#pub(self) }"
  if [[ "$body" == pub\(* ]]; then
    body="${body#pub(}"
    if [[ "$body" == *') '* ]]; then
      body="${body#*) }"
    else
      body="$t"
    fi
  fi
  body="${body#pub }"

  case "$body" in
    mod\ *|use\ *|extern\ crate\ *) return 0 ;;
    type\ *|struct\ *|enum\ *|union\ *|trait\ *|impl\ *) return 0 ;;
    static\ *) return 0 ;;
    const\ fn\ *) return 1 ;;
    const\ *) return 0 ;;
  esac

  # Trait / extern method signatures without a body.
  if [[ "$body" == fn\ * && "$t" == *';' && "$t" != *'{'* ]]; then
    return 0
  fi

  # Enum unit/tuple variants and simple `name: Type` fields.
  if [[ "$t" =~ ^[[:space:]]*[A-Z][A-Za-z0-9_,\(\)]*[[:space:]]*,?[[:space:]]*$ ]]; then
    return 0
  fi
  local field="${body%,}"
  if [[ "$field" == *:* && "$field" != *::* && "$field" != *=* && "$field" != *'{'* ]]; then
    case "$field" in
      fn\ *|if\ *|match\ *|for\ *|while\ *|return\ *|let\ *) ;;
      *) return 0 ;;
    esac
  fi

  return 1
}

# 1-based line numbers inside top-level #[cfg(test)] items.
cfg_test_lines() {
  local source_file="$1"
  awk '
    function strip_comment(line,   i, in_s, ch, out) {
      in_s = 0
      out = ""
      for (i = 1; i <= length(line); i++) {
        ch = substr(line, i, 1)
        if (ch == "\"" && !in_s) { in_s = 1; out = out ch; continue }
        if (ch == "\"" && in_s)  { in_s = 0; out = out ch; continue }
        if (in_s) {
          out = out ch
          if (ch == "\\") { i++; if (i <= length(line)) out = out substr(line, i, 1) }
          continue
        }
        if (ch == "/" && substr(line, i + 1, 1) == "/") break
        out = out ch
      }
      return out
    }
    function update_depth(line, depth,   i, ch, in_s, in_c, in_b) {
      in_s = 0; in_c = 0; in_b = 0
      for (i = 1; i <= length(line); i++) {
        ch = substr(line, i, 1)
        if (in_b) {
          if (ch == "*" && substr(line, i + 1, 1) == "/") { in_b = 0; i++ }
          continue
        }
        if (in_s) {
          if (ch == "\\") { i++; continue }
          if (ch == "\"") in_s = 0
          continue
        }
        if (in_c) {
          if (ch == "\\") { i++; continue }
          if (ch == "'\''") in_c = 0
          continue
        }
        if (ch == "\"") { in_s = 1; continue }
        if (ch == "'\''") { in_c = 1; continue }
        if (ch == "/" && substr(line, i + 1, 1) == "*") { in_b = 1; i++; continue }
        if (ch == "/" && substr(line, i + 1, 1) == "/") break
        if (ch == "{") depth++
        else if (ch == "}") { depth--; if (depth < 0) depth = 0 }
      }
      return depth
    }
    function is_attr(t) { return (t ~ /^#\[[^\]]*\]/) }
    function is_cfg_test(t,   inner) {
      if (t !~ /^#\[[[:space:]]*cfg[[:space:]]*\([[:space:]]*test[[:space:]]*\)[[:space:]]*\]$/) return 0
      return 1
    }
    function looks_item(t) {
      return (t ~ /^(pub([[:space:]]|\([^)]*\)[[:space:]]*))?(mod|fn|struct|enum|union|trait|impl|type|const|static|async[[:space:]]+fn|unsafe[[:space:]]+(fn|impl))[[:space:]]/)
    }
    {
      raw[NR] = $0
    }
    END {
      depth = 0
      i = 1
      while (i <= NR) {
        trimmed = strip_comment(raw[i])
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", trimmed)
        if (depth == 0 && is_attr(trimmed)) {
          attr_start = i
          j = i
          saw = 0
          while (j <= NR) {
            t = strip_comment(raw[j])
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", t)
            if (t == "") { j++; continue }
            if (!is_attr(t)) break
            if (is_cfg_test(t)) saw = 1
            j++
          }
          while (j <= NR) {
            t = strip_comment(raw[j])
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", t)
            if (t != "") break
            j++
          }
          if (saw && j <= NR) {
            item = strip_comment(raw[j])
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", item)
            end = 0
            if (item !~ /\{/ && item ~ /;$/) {
              end = j
            } else if (item ~ /\{/ || looks_item(item)) {
              d = depth
              seen_open = 0
              for (k = j; k <= NR; k++) {
                before = d
                d = update_depth(raw[k], d)
                if (d > before) seen_open = 1
                if (seen_open && d == depth) { end = k; break }
                if (!seen_open && k > j + 32) break
              }
            }
            if (end > 0) {
              for (k = attr_start; k <= end; k++) print k
              i = end + 1
              continue
            }
          }
          i++
          continue
        }
        depth = update_depth(raw[i], depth)
        i++
      }
    }
  ' "$source_file"
}

print_footer() {
  local status="$1"
  if [[ -n "$ARTIFACT_HINT" ]]; then
    echo "reports: HTML artifact \`$HTML_ARTIFACT\`, JSON artifact \`$JSON_ARTIFACT\`, LCOV artifact \`$LCOV_ARTIFACT\` ($ARTIFACT_HINT)"
  else
    echo "reports: HTML artifact \`$HTML_ARTIFACT\`, JSON artifact \`$JSON_ARTIFACT\`, LCOV artifact \`$LCOV_ARTIFACT\`"
  fi
  echo "result: $status"
}

# ---------------------------------------------------------------------------
# overall
# ---------------------------------------------------------------------------

run_overall() {
  local ws_stats nsb_stats
  local ws_covered ws_count ws_pct
  local nsb_covered nsb_count nsb_pct
  local failed=0
  local cli_stats tools_stats

  ws_stats="$(workspace_line_stats)"
  read -r ws_covered ws_count ws_pct <<<"$ws_stats"

  nsb_stats="$(crate_line_stats nsb)"
  cli_stats="$(crate_line_stats nsb-cli)"
  tools_stats="$(crate_line_stats nsb-data-tools)"

  echo "NSB coverage gate (overall)"
  echo "baseline_kind: $BASELINE_KIND"
  echo "baseline: $BASELINE_COMMIT ($BASELINE_DATE)"

  printf 'workspace lines: %.2f%% (floor %.2f%%; measured baseline %.2f%%)\n' \
    "$ws_pct" "$FLOOR_WS" "$MEASURED_WS"

  if [[ "$nsb_stats" == "missing" ]]; then
    printf 'nsb lines: missing (floor %.2f%%; measured baseline %.2f%%)\n' \
      "$FLOOR_NSB" "$MEASURED_NSB"
  else
    read -r nsb_covered nsb_count nsb_pct <<<"$nsb_stats"
    printf 'nsb lines: %.2f%% (floor %.2f%%; measured baseline %.2f%%)\n' \
      "$nsb_pct" "$FLOOR_NSB" "$MEASURED_NSB"
  fi

  if [[ "$cli_stats" == "missing" ]]; then
    echo "nsb-cli lines: missing (recorded, not a separate floor)"
  else
    read -r _ _ cli_pct <<<"$cli_stats"
    printf 'nsb-cli lines: %.2f%% (recorded, not a separate floor)\n' "$cli_pct"
  fi
  if [[ "$tools_stats" == "missing" ]]; then
    echo "nsb-data-tools lines: missing (recorded, not a separate floor)"
  else
    read -r _ _ tools_pct <<<"$tools_stats"
    printf 'nsb-data-tools lines: %.2f%% (recorded, not a separate floor)\n' "$tools_pct"
  fi
  echo "functions/regions: not evaluated by this gate (line floors are blocking; see coverage JSON artifact for diagnostics)"

  if [[ "$ws_count" -eq 0 ]]; then
    failed=1
    echo "FAIL: coverage report contains no instrumented lines"
  fi
  if [[ "$nsb_stats" == "missing" ]]; then
    failed=1
    echo "FAIL: no nsb coverage data in the report (fail-closed; missing crates/nsb files or instrumented lines)"
  fi
  if [[ "$ws_count" -gt 0 ]]; then
    if awk -v p="$ws_pct" -v f="$FLOOR_WS" 'BEGIN { exit !(p + 1e-9 < f) }'; then
      failed=1
      printf 'FAIL: workspace line coverage %.2f%% is below the floor %.2f%%\n' "$ws_pct" "$FLOOR_WS"
    fi
  fi
  if [[ "$nsb_stats" != "missing" ]]; then
    if awk -v p="$nsb_pct" -v f="$FLOOR_NSB" 'BEGIN { exit !(p + 1e-9 < f) }'; then
      failed=1
      printf 'FAIL: nsb line coverage %.2f%% is below the floor %.2f%%\n' "$nsb_pct" "$FLOOR_NSB"
    fi
  fi

  if [[ "$failed" -eq 0 ]]; then
    print_footer PASS
    return 0
  fi
  print_footer FAIL
  return 1
}

# ---------------------------------------------------------------------------
# diff
# ---------------------------------------------------------------------------

# Parse git diff -U0 into path<TAB>line<TAB>text for added lines.
parse_unified_diff() {
  awk '
    BEGIN { path = ""; new_line = 0; have_line = 0 }
    /^\+\+\+ b\// {
      path = substr($0, 7)
      gsub(/\r$/, "", path)
      next
    }
    /^\+\+\+ \/dev\/null/ { path = ""; next }
    /^@@ / {
      # @@ -old[,len] +new[,len] @@
      for (i = 1; i <= NF; i++) {
        if ($i ~ /^\+[0-9]/) {
          split(substr($i, 2), r, ",")
          new_line = r[1] + 0
          have_line = 1
          break
        }
      }
      next
    }
    /^diff |^index |^--- / { next }
    {
      if (path == "" || !have_line) next
      if ($0 ~ /^\+/) {
        text = substr($0, 2)
        printf "%s\t%d\t%s\n", path, new_line, text
        new_line++
      } else if ($0 ~ /^-/) {
        # deleted: old file only
      } else if ($0 ~ /^\\/) {
        # no newline marker
      } else {
        new_line++
      }
    }
  '
}

changed_lines_from_git() {
  local base="$1"
  local merge_base range
  merge_base="$(git merge-base "$base" HEAD)" || die "git merge-base $base HEAD failed"
  [[ -n "$merge_base" ]] || die "git merge-base $base HEAD produced an empty SHA"
  range="${merge_base}...HEAD"
  git diff -U0 --no-color --find-renames "$range" -- '*.rs'
}

run_diff() {
  local failed=0
  local ignored_test_files=0
  local ignored_inline_test=0
  local executable=0
  local covered=0
  local percent
  local uncovered_list=()
  local missing_files=()

  CHANGED_TMP="$(mktemp)"
  CFG_TMP="$(mktemp)"
  PROD_TMP="$(mktemp)"

  if [[ -n "$DIFF_FILE" ]]; then
    [[ -f "$DIFF_FILE" ]] || die "diff file not found: $DIFF_FILE"
    parse_unified_diff <"$DIFF_FILE" >"$CHANGED_TMP"
  else
    local base_ref="${BASE:-$DEFAULT_BASE}"
    if [[ -z "$BASE" && -n "${GITHUB_BASE_REF:-}" ]]; then
      base_ref="origin/${GITHUB_BASE_REF}"
    fi
    changed_lines_from_git "$base_ref" | parse_unified_diff >"$CHANGED_TMP"
  fi

  # Deduplicate path+line (keep first text).
  awk -F '\t' '
    !seen[$1 FS $2]++ { print }
  ' "$CHANGED_TMP" | LC_ALL=C sort -t $'\t' -k1,1 -k2,2n >"$PROD_TMP"
  mv "$PROD_TMP" "$CHANGED_TMP"

  local current_path=""
  local -a path_lines=()
  local -a path_texts=()

  flush_path() {
    local path="$1"
    [[ -n "$path" ]] || return 0
    if ! is_production_rust_file "$path"; then
      ignored_test_files=$((ignored_test_files + 1))
      return 0
    fi

    local -A cfg_exclude=()
    local source_file="$SOURCE_ROOT/$path"
    if [[ -f "$source_file" ]]; then
      while IFS= read -r ln; do
        cfg_exclude["$ln"]=1
      done < <(cfg_test_lines "$source_file")
    fi

    local -a prod_lines=()
    local -a prod_texts=()
    local i
    for i in "${!path_lines[@]}"; do
      local line="${path_lines[$i]}"
      if [[ -n "${cfg_exclude[$line]+x}" ]]; then
        ignored_inline_test=$((ignored_inline_test + 1))
        continue
      fi
      prod_lines+=("$line")
      prod_texts+=("${path_texts[$i]}")
    done
    [[ ${#prod_lines[@]} -gt 0 ]] || return 0

    # Does LCOV contain this file?
    if ! awk -F '\t' -v p="$path" '$1 == p { found=1; exit } END { exit !found }' "$LCOV_TABLE"; then
      local all_decl=1
      for i in "${!prod_lines[@]}"; do
        if ! is_non_instrumentable_line "${prod_texts[$i]}"; then
          all_decl=0
          break
        fi
      done
      if [[ "$all_decl" -eq 0 ]]; then
        missing_files+=("$path")
      fi
      return 0
    fi

    for i in "${!prod_lines[@]}"; do
      local line="${prod_lines[$i]}"
      local hits
      hits="$(awk -F '\t' -v p="$path" -v l="$line" '
        $1 == p && $2 + 0 == l + 0 { print $3 + 0; found=1; exit }
        END { if (!found) print "" }
      ' "$LCOV_TABLE")"
      if [[ -z "$hits" ]]; then
        # No DA record → non-executable / not instrumented.
        continue
      fi
      executable=$((executable + 1))
      if [[ "$hits" -eq 0 ]]; then
        uncovered_list+=("${path}:${line}")
      else
        covered=$((covered + 1))
      fi
    done
  }

  while IFS= read -r raw || [[ -n "${raw:-}" ]]; do
    [[ -n "${raw:-}" ]] || continue
    path="${raw%%$'\t'*}"
    rest="${raw#*$'\t'}"
    line="${rest%%$'\t'*}"
    text="${rest#*$'\t'}"
    if [[ "$path" != "$current_path" ]]; then
      flush_path "$current_path"
      current_path="$path"
      path_lines=()
      path_texts=()
    fi
    path_lines+=("$line")
    path_texts+=("$text")
  done <"$CHANGED_TMP"
  flush_path "$current_path"

  if [[ "$executable" -eq 0 ]]; then
    percent="100.00"
  else
    percent="$(awk -v c="$covered" -v e="$executable" 'BEGIN { printf "%.10f", (c * 100.0) / e }')"
  fi

  echo "NSB coverage gate (diff)"
  echo "baseline_kind: $BASELINE_KIND"
  echo "baseline: $BASELINE_COMMIT ($BASELINE_DATE)"
  printf 'diff production lines: %.2f%% (%d/%d executable changed lines; floor %.2f%%)\n' \
    "$percent" "$covered" "$executable" "$FLOOR_DIFF"
  echo "ignored non-production/test Rust files: $ignored_test_files"
  echo "ignored inline #[cfg(test)] lines: $ignored_inline_test"

  if [[ ${#missing_files[@]} -gt 0 ]]; then
    failed=1
    echo "FAIL: ${#missing_files[@]} changed production file(s) have no coverage information"
  fi
  if awk -v p="$percent" -v f="$FLOOR_DIFF" 'BEGIN { exit !(p + 1e-9 < f) }'; then
    failed=1
    printf 'FAIL: changed production line coverage %.2f%% is below the floor %.2f%%\n' \
      "$percent" "$FLOOR_DIFF"
  fi
  if [[ "$executable" -eq 0 && ${#missing_files[@]} -eq 0 ]]; then
    echo "no executable changed production lines; diff gate passes"
  fi

  if [[ ${#uncovered_list[@]} -gt 0 ]]; then
    echo "uncovered changed production lines:"
    local u
    for u in "${uncovered_list[@]}"; do
      echo "  $u"
    done
  fi
  if [[ ${#missing_files[@]} -gt 0 ]]; then
    echo "changed production files with no coverage data:"
    local m
    for m in "${missing_files[@]}"; do
      echo "  $m"
    done
  fi

  if [[ "$failed" -eq 0 ]]; then
    print_footer PASS
    return 0
  fi
  print_footer FAIL
  return 1
}

case "$COMMAND" in
  overall) run_overall ;;
  diff) run_diff ;;
esac
