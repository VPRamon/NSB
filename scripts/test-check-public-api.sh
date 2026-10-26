#!/usr/bin/env bash
# Focused lifecycle tests; all snapshot mutations stay in a temporary Git repo.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "$ROOT/scripts/tests/test_api_migration.py"
FIXTURE="$(mktemp -d)"
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/scripts" "$FIXTURE/crates/nsb/api" "$FIXTURE/bin"
cp "$ROOT/scripts/check-public-api.sh" "$ROOT/scripts/check-api-migration.py" "$FIXTURE/scripts/"
cp "$ROOT/crates/nsb/api/target-direction-migration.json" "$FIXTURE/crates/nsb/api/"
printf 'pub struct nsb::Example;\n' > "$FIXTURE/crates/nsb/api/public-api.txt"
cp "$FIXTURE/crates/nsb/api/public-api.txt" "$FIXTURE/generated"
cd "$FIXTURE"
git init -q
git config user.name 'API lifecycle fixture'
git config user.email 'api-fixture@example.invalid'
git add crates
git commit -qm 'non-frozen base'
BOOTSTRAP="$(git rev-parse HEAD)"
touch crates/nsb/api/API_FROZEN
git add crates
git commit -qm 'frozen base'
FROZEN="$(git rev-parse HEAD)"
cp generated crates/nsb/api/public-api.txt
git add crates
git commit --allow-empty -qm 'current snapshot'
NEW="$(git rev-parse HEAD)"
cat > bin/cargo <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" == 'public-api --version' ]]; then
  echo 'cargo-public-api 0.50.1'
elif [[ "$*" == *'public-api diff'* ]]; then
  touch diff-called
  [[ "$*" == *"--deny=changed"* ]]
  exit "${API_DIFF_STATUS:-0}"
else
  cat generated
fi
MOCK
chmod +x bin/cargo
export PATH="$FIXTURE/bin:$PATH"
unset NSB_PUBLIC_API_BASE
SCRIPT=scripts/check-public-api.sh
expect_failure() {
  if "$SCRIPT" "$@" >result 2>&1; then
    echo "FAIL: expected failure: $*" >&2; exit 1
  fi
}
expect_failure
expect_failure --base ''
expect_failure --base HEAD
expect_failure --base definitely-not-a-revision
"$SCRIPT" --base "$BOOTSTRAP" >result
grep -q freeze-bootstrap result
"$SCRIPT" --base "$FROZEN" >result
test -f diff-called
API_DIFF_STATUS=1 expect_failure --base "$FROZEN"
# A current reviewed baseline still invokes the real SemVer path.
git commit --allow-empty -qm 'subsequent change'
"$SCRIPT" --base "$NEW" >result
test -f diff-called
API_DIFF_STATUS=1 expect_failure --base "$NEW"
# Generated/saved snapshot drift fails before any historical comparison.
printf '\npub struct nsb::Unexpected;\n' >> generated
expect_failure --base "$FROZEN"
grep -q 'snapshot drift' result
# Even a matching edited snapshot cannot bypass the SemVer check.
cp generated crates/nsb/api/public-api.txt
API_DIFF_STATUS=1 expect_failure --base "$FROZEN"
grep -q 'SemVer gate: cargo public-api diff' result
# A matching snapshot cannot authorize an unrelated signature change.
printf 'pub struct nsb::Example(u8);\n' > generated
cp generated crates/nsb/api/public-api.txt
expect_failure --base "$FROZEN"
grep -q 'unapproved removed/changed public API' result
# --write is exercised only here, within the disposable fixture.
"$SCRIPT" --write >result
cmp generated crates/nsb/api/public-api.txt
# Forbidden debt is rejected even if the snapshot agrees.
printf 'pub nsb::ThresholdQuery::sample_step: qtty::Second\n' >> generated
cp generated crates/nsb/api/public-api.txt
expect_failure --base "$FROZEN"
grep -q 'forbidden public API' result
echo 'all public API lifecycle assertions passed (checkout snapshot unchanged)'
