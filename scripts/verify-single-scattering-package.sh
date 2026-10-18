#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
# Verify the real Cargo archive, with an explicit negative registry-build check.
# Never represent the temporary archive as registry-publishable.
set -euo pipefail
revision=83d1ebb29ca1ae0b327d5784d13eda231d52c120
version=$(python3 -c 'import tomllib; print(tomllib.load(open("crates/nsb/Cargo.toml", "rb"))["package"]["version"])')
log=$(mktemp)
trap 'rm -f "$log"' EXIT
if cargo package -p nsb --locked "$@" >"$log" 2>&1; then
  cat "$log"
  echo 'Registry packaging unexpectedly succeeded: reassess and remove the temporary release blocker.' >&2
  exit 1
fi
cat "$log"
# A network, lockfile, license, source or unrelated compiler error is not an
# acceptable substitute for the specifically established upstream API blocker.
grep -Fq 'unresolved imports `siderust::atmosphere::LayeredAtmosphere`, `siderust::atmosphere::NormalizedPhaseTable`' "$log"
grep -Fq 'failed to verify package tarball' "$log"
echo 'Registry publication BLOCKED: Siderust 0.14.1 lacks the required APIs.'
archive="target/package/nsb-$version/Cargo.toml"
# Rebuild the extracted archive, not the working tree, with the documented pin.
cargo check --manifest-path "$archive" --all-targets --all-features \
  --config "patch.crates-io.siderust.git='https://github.com/Siderust/siderust'" \
  --config "patch.crates-io.siderust.rev='$revision'"
echo "Archive verified with temporary Siderust revision $revision; NOT registry-publishable."
