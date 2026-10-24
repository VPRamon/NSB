#!/usr/bin/env bash
# Build the local NSB checkout (this repository) into a verified wheel + receipt.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON="${NSB_CTAO_PYTHON:-${ROOT}/.venv/bin/python}"
WORK="${NSB_CTAO_BUILD_ROOT:-${ROOT}/ctao/.build}"
SOURCE="${NSB_SOURCE:-$ROOT}"

if [[ ! -d "$SOURCE/.git" ]]; then
  echo "ERROR: NSB source checkout not found at $SOURCE" >&2
  exit 1
fi
if [[ ! -x "$PYTHON" ]]; then
  echo "ERROR: Python not found at $PYTHON (create a venv first)" >&2
  exit 1
fi

PIN_COMMIT="$(git -C "$SOURCE" rev-parse HEAD)"
if [[ -n "$(git -C "$SOURCE" status --porcelain)" ]]; then
  echo "WARNING: NSB source has uncommitted changes; receipt will mark source_clean=false" >&2
  CLEAN=0
else
  CLEAN=1
fi

mkdir -p "$WORK"
OUT="$WORK/nsb-${PIN_COMMIT:0:12}"
rm -rf "$OUT"

if command -v uv >/dev/null 2>&1; then
  uv pip install --python "$PYTHON" "siderust>=0.5.1,<0.6" "tempoch>=0.5.0,<0.6" maturin==1.15.0
else
  "$PYTHON" -m pip install "siderust>=0.5.1,<0.6" "tempoch>=0.5.0,<0.6" maturin==1.15.0
fi

echo "==> Building NSB $PIN_COMMIT"
mkdir -p "$OUT/wheels"
export CARGO_TARGET_DIR="$WORK/cargo-target"
export PYO3_PYTHON="$PYTHON"
( cd "$SOURCE" && uvx --from maturin==1.15.0 maturin build --release --locked \
    --features python --manifest-path crates/nsb/Cargo.toml --out "$OUT/wheels" )

WHEEL="$(ls "$OUT/wheels"/*.whl)"
if command -v uv >/dev/null 2>&1; then
  uv pip install --python "$PYTHON" --reinstall "$WHEEL"
else
  "$PYTHON" -m pip install --force-reinstall "$WHEEL"
fi

"$PYTHON" - <<PY
import hashlib, json, nsb
from importlib.metadata import distribution
from pathlib import Path
wheel = Path(${WHEEL@Q})
package = distribution("nsb-py")
extensions = {
    str(f): hashlib.sha256(Path(str(package.locate_file(f))).read_bytes()).hexdigest()
    for f in package.files
    if str(f).endswith((".so", ".pyd"))
}
receipt = {
    "git_commit": ${PIN_COMMIT@Q},
    "source_clean": True if int("""${CLEAN}""") == 1 else False,
    "wheel_sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
    "extension_sha256": extensions,
    "cargo_lock_sha256": hashlib.sha256(Path(${SOURCE@Q}, "Cargo.lock").read_bytes()).hexdigest(),
}
assert nsb.__version__.startswith("0.2")
# Automatic must resolve to Oxygen for this release candidate.
cfg = nsb.NsbModelConfig.generic_clear_sky()
assert cfg.airglow_selection == "automatic"
oxy = nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1
cont = nsb.AirglowModel.PARANAL_PALACE_V1
assert oxy.as_str() == "paranal-palace-oxygen-v1"
assert cont.as_str() == "paranal-palace-v1"
Path(${OUT@Q}, "nsb-build-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
print("NSB OK", nsb.__version__, receipt["git_commit"][:12], "receipt written")
# Fail closed if Automatic is still continuum-only: construct evaluators and
# compare airglow band integrals — Automatic must match explicit Oxygen.
import siderust
from datetime import UTC, datetime
observer = siderust.Observer(lon_deg=-70.404, lat_deg=-24.627, height_m=2150.0)
instant = datetime(2023, 9, 4, 1, 48, tzinfo=UTC)
# Near-zenith ICRS target so Automatic Oxygen (ZD ≤ 60°) evaluates.
target = siderust.Direction(ra_deg=0.0, dec_deg=-24.627)
query = nsb.PointQuery(observer, instant, target, components=nsb.ComponentMask.AIRGLOW)
auto = float(nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky()).integrate_band(query, 365.0, 650.0))
explicit = float(
    nsb.NsbEvaluator(
        nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(oxy)
    ).integrate_band(query, 365.0, 650.0)
)
continuum = float(
    nsb.NsbEvaluator(
        nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(cont)
    ).integrate_band(query, 365.0, 650.0)
)
assert abs(auto - explicit) <= 1e-12 * max(1.0, abs(explicit)), (auto, explicit)
assert auto > continuum, (auto, continuum)
print("Automatic==Oxygen OK; Oxygen>continuum OK")
PY
echo "$OUT/nsb-build-receipt.json"
