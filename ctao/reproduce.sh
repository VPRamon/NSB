#!/usr/bin/env bash
# Reproduce the CTAO validation notebook against this NSB checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-lightweight}"
case "$MODE" in
  lightweight|full) ;;
  *) echo "Usage: $0 [lightweight|full]" >&2; exit 2 ;;
esac

export NSB_CTAO_MODE="$MODE"
export NSB_CTAO_SEED="${NSB_CTAO_SEED:-20260324}"
export NSB_CTAO_BUILD_ROOT="${NSB_CTAO_BUILD_ROOT:-$ROOT/ctao/.build}"
export PYTHONPATH="${ROOT}/ctao/python${PYTHONPATH:+:$PYTHONPATH}"

if [[ -z "${NSB_CTAO_PYTHON:-}" ]]; then
  if [[ ! -x "$ROOT/.venv/bin/python" ]]; then
    echo "==> Creating project venv"
    if command -v uv >/dev/null 2>&1; then
      uv venv "$ROOT/.venv"
    else
      python3 -m venv "$ROOT/.venv"
    fi
  fi
  NSB_CTAO_PYTHON="$ROOT/.venv/bin/python"
fi
export NSB_CTAO_PYTHON
PYTHON="$NSB_CTAO_PYTHON"

install_py() {
  if command -v uv >/dev/null 2>&1; then
    uv pip install --python "$PYTHON" "$@"
  else
    "$PYTHON" -m pip install "$@"
  fi
}

echo "==> Python: $PYTHON"
echo "==> Mode:   $MODE"
echo "==> Root:   $ROOT"

install_py nbclient nbformat matplotlib numpy astropy healpy ipykernel \
  "siderust>=0.5.1,<0.6" "tempoch>=0.5.0,<0.6"
"$PYTHON" -m ipykernel install --prefix="$ROOT/.venv" --name=python3 --display-name="Python 3" >/dev/null
export JUPYTER_PATH="${ROOT}/.venv/share/jupyter${JUPYTER_PATH:+:$JUPYTER_PATH}"
export JUPYTER_DATA_DIR="${ROOT}/.venv/share/jupyter"

# Build/install this checkout's NSB if needed
RECEIPT_DEFAULT="$NSB_CTAO_BUILD_ROOT/nsb-$(git -C "$ROOT" rev-parse --short=12 HEAD)/nsb-build-receipt.json"
# Also accept any receipt matching HEAD
HEAD="$(git -C "$ROOT" rev-parse HEAD)"
RECEIPT_DEFAULT="$NSB_CTAO_BUILD_ROOT/nsb-${HEAD:0:12}/nsb-build-receipt.json"
export NSB_BUILD_RECEIPT="${NSB_BUILD_RECEIPT:-$RECEIPT_DEFAULT}"

NEED_BUILD=0
if [[ ! -f "$NSB_BUILD_RECEIPT" ]]; then
  NEED_BUILD=1
elif ! "$PYTHON" -c "import nsb; assert nsb.__version__.startswith('0.2')" 2>/dev/null; then
  NEED_BUILD=1
else
  # Fail if Automatic still equals continuum-only
  if ! "$PYTHON" - <<'PY'
import nsb, siderust
from datetime import UTC, datetime
observer = siderust.Observer(lon_deg=-70.404, lat_deg=-24.627, height_m=2150.0)
instant = datetime(2023, 9, 4, 1, 48, tzinfo=UTC)
target = siderust.Direction(ra_deg=0.0, dec_deg=-24.627)
query = nsb.PointQuery(observer, instant, target, components=nsb.ComponentMask.AIRGLOW)
auto = float(nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky()).integrate_band(query, 365.0, 650.0))
oxy = float(nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1)).integrate_band(query, 365.0, 650.0))
assert abs(auto - oxy) <= 1e-12 * max(1.0, abs(oxy))
PY
  then
    NEED_BUILD=1
  fi
fi

if [[ "$NEED_BUILD" == "1" ]]; then
  echo "==> Building NSB from this checkout"
  chmod +x "$ROOT/ctao/install_nsb.sh"
  "$ROOT/ctao/install_nsb.sh"
fi
export NSB_BUILD_RECEIPT

if [[ "$MODE" == "full" ]]; then
  if ! "$PYTHON" -c "import nsb2, importlib.metadata as m, json; d=json.loads(m.distribution('nsb2').read_text('direct_url.json') or '{}'); assert d.get('vcs_info',{}).get('commit_id')=='bc9320db03fbff69997ce366c6e76a37339c5d00'" 2>/dev/null; then
    echo "==> Installing pinned nsb2"
    install_py "nsb2 @ git+https://github.com/GerritRo/nsb2.git@bc9320db03fbff69997ce366c6e76a37339c5d00"
  fi
fi

NB="$ROOT/notebooks/NSB_CTAO_Validation.ipynb"
OUT_DIR="$ROOT/ctao/results"
mkdir -p "$OUT_DIR"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EXECUTED="$OUT_DIR/NSB_CTAO_Validation_${MODE}_${STAMP}.ipynb"

echo "==> Executing $NB ($MODE)"
"$PYTHON" - <<PY
from pathlib import Path
import nbformat
from nbclient import NotebookClient
nb = nbformat.read(Path(${NB@Q}).open(), as_version=4)
client = NotebookClient(
    nb, timeout=7200, kernel_name="python3",
    resources={"metadata": {"path": str(Path(${ROOT@Q}))}},
)
client.execute()
nbformat.write(nb, Path(${EXECUTED@Q}).open("w"))
print("Wrote", ${EXECUTED@Q})
PY

echo "==> Done: $EXECUTED"
