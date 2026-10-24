#!/usr/bin/env python3
"""Build notebooks/NSB_CTAO_Validation.ipynb (executable scientific article)."""

from __future__ import annotations

import json
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "notebooks" / "NSB_CTAO_Validation.ipynb"


def md(source: str) -> dict:
    return {
        "cell_type": "markdown",
        "id": str(uuid.uuid4()),
        "metadata": {},
        "source": _lines(source),
    }


def code(source: str) -> dict:
    return {
        "cell_type": "code",
        "id": str(uuid.uuid4()),
        "execution_count": None,
        "metadata": {},
        "outputs": [],
        "source": _lines(source),
    }


def _lines(text: str) -> list[str]:
    text = text.strip("\n") + "\n"
    parts = text.split("\n")
    return [p + "\n" for p in parts[:-1]] + ([parts[-1] + "\n"] if parts[-1] else [])


CELLS = [
    md(
        """# NSB v0.2.0 — CTAO Scientific Validation Notebook

**Audience:** Cherenkov Telescope Array Observatory (CTAO) scientists and software engineers evaluating [NSB](https://github.com/VPRamon/NSB) against [nsb2](https://github.com/GerritRo/nsb2).

**Primary experiment:** complete **four-component** night-sky photon radiance (zodiacal + airglow + starlight + moonlight=0) versus absolute Patat/FORS1 observations over **365–650 nm**, on the registered **94 evaluation spectra / 17 nights**.

**v0.2.0 Automatic Airglow:** PALACE Oxygen (release-policy default). Explicit continuum remains available.
**Scientific admission:** independent absolute Oxygen holdout remains absent (`LVM_ABSOLUTE_EQUIVALENCE = PENDING_EXTERNAL_EVIDENCE`). Protected LVM spectra remain sealed. Patat is **development evidence**, not an independent holdout. The default change does **not** rewrite that NO-GO admission.

| Mode | Command | Scope |
|------|---------|-------|
| lightweight | `./ctao/reproduce.sh lightweight` | Provenance, physical checks, OI gate, CTAO scenarios |
| **full** | `./ctao/reproduce.sh full` | **+ four-component Patat benchmark (continuum, Oxygen, nsb2, baseline)** |

All numeric claims below are produced by **this run**. Historical MAE values appear only as reference checks.
"""
    ),
    md(
        """## 1 · Scientific objective

NSB predicts ground-level night-sky spectral photon radiance from physical components for clear-sky sites. CTAO needs credible rates for thresholds, exposure planning and background systematics.

**Success** means honest, reproducible agreement of the **complete** model with admitted observations—and a fair paired comparison with nsb2—not a manufactured win.

Airglow-only predictions versus **total** observed sky are **not** a valid primary accuracy metric (they omit zodiacal light and starlight). That diagnostic is demoted to a supplement.
"""
    ),
    md("## 2 · Environment and provenance"),
    code(
        r"""
import json
import os
import sys
from datetime import UTC, datetime
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np

def find_root() -> Path:
    env = os.environ.get("NSB_ROOT")
    if env:
        return Path(env).expanduser().resolve()
    here = Path.cwd().resolve()
    for candidate in [here, *here.parents]:
        if (candidate / "crates" / "nsb").is_dir() and (
            candidate / "ctao" / "configs" / "patat-fors1-spectral-v1.json"
        ).is_file():
            return candidate
    raise FileNotFoundError("Cannot locate NSB root. Set NSB_ROOT to the NSB checkout.")

ROOT = find_root()
sys.path.insert(0, str(ROOT / "ctao" / "python"))

MODE = os.environ.get("NSB_CTAO_MODE", "lightweight").strip().lower()
if MODE not in {"lightweight", "full"}:
    raise ValueError(f"Unknown NSB_CTAO_MODE={MODE!r}")
SEED = int(os.environ.get("NSB_CTAO_SEED", "20260324"))
RUN_ID = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
RESULTS = Path(os.environ.get("NSB_CTAO_RESULTS", ROOT / "ctao" / "results" / RUN_ID))
RESULTS.mkdir(parents=True, exist_ok=True)
FIGDIR = RESULTS / "figures"
FIGDIR.mkdir(exist_ok=True)
RECEIPT = os.environ.get("NSB_BUILD_RECEIPT")
RECEIPT_PATH = Path(RECEIPT).expanduser().resolve() if RECEIPT else None

from nsb_ctao.provenance import collect_run_manifest, write_json
from nsb_ctao.four_component import PINNED_NSB2_COMMIT

import subprocess
_nsb_head = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
MANIFEST = collect_run_manifest(repo_root=ROOT, mode=MODE, seed=SEED, extra={
    "nsb_checkout_commit": _nsb_head,
    "pinned_nsb2_commit": PINNED_NSB2_COMMIT,
    "nsb_build_receipt": str(RECEIPT_PATH) if RECEIPT_PATH else None,
    "automatic_airglow_policy": "ParanalPalaceOxygenV1",
})
MANIFEST["results_dir_name"] = RESULTS.name
write_json(RESULTS / "run_manifest.json", MANIFEST)
print(f"Mode: {MODE}\nResults: {RESULTS}")
print(json.dumps({k: MANIFEST[k] for k in (
    "executed_at_utc", "nsb", "nsb2", "validation_package",
    "lvm_absolute_equivalence", "oxygen_default_admission", "extra"
)}, indent=2, default=str))
"""
    ),
    md(
        """## 3 · Physical model verification

Numerical consistency of the installed NSB package — **not** observational validation.
"""
    ),
    code(
        r"""
from nsb_ctao.physical_checks import run_physical_checks

PHYSICAL = run_physical_checks(f107_sfu=130.0)
write_json(RESULTS / "physical_checks.json", PHYSICAL)
print("all_passed:", PHYSICAL["all_passed"])
for check in PHYSICAL["checks"]:
    print(f"  [{check['status']}] {check['id']}")
assert PHYSICAL["all_passed"], "Physical checks failed"

# v0.2.0 policy: Automatic must match explicit Oxygen numerically.
import nsb, siderust
from datetime import datetime
_obs = siderust.Observer(lon_deg=-70.404, lat_deg=-24.627, height_m=2150.0)
_t = datetime(2023, 9, 4, 1, 48, tzinfo=UTC)
_q = nsb.PointQuery(_obs, _t, siderust.Direction(ra_deg=0.0, dec_deg=-24.627), components=nsb.ComponentMask.AIRGLOW)
_auto = float(nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky()).integrate_band(_q, 365.0, 650.0))
_oxy = float(nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1)).integrate_band(_q, 365.0, 650.0))
_cont = float(nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_V1)).integrate_band(_q, 365.0, 650.0))
assert abs(_auto - _oxy) <= 1e-12 * max(1.0, abs(_oxy))
assert _auto > _cont
print(f"Automatic==Oxygen OK ({_auto:.6f}); continuum={_cont:.6f}")

"""
    ),
    md(
        """## 4 · Full four-component Patat/FORS1 benchmark (**primary**)

### Observable
Ground-level photon surface radiance, 365–650 nm, moonless FORS1 spectra (Patat 2008).

### Cohort
Registered selection → **103 spectra / 22 nights**; preregistered night split → **94 evaluation / 17 nights** (5 calibration nights for the median baseline only).

### Models (same cohort, band, geometry)
| ID | Configuration |
|----|----------------|
| A | NSB v0.2.0 complete, **explicit continuum**, native solar |
| B | NSB v0.2.0 complete, **Automatic/Oxygen**, native solar |
| C | A with measured centered 27-day F10.7 |
| D | B with measured centered 27-day F10.7 |
| E | nsb2 complete, header SUNFLUX |
| F | nsb2 complete, matched 27-day F10.7 |
| G | Median baseline (fit on calibration nights only) |

**Status:** development retrospective — not an independent Oxygen holdout.

Full mode required. Lightweight mode skips this section without inheriting its conclusions.
"""
    ),
    code(
        r"""
from IPython.display import Markdown, display

FOUR = None
if MODE == "full":
    from nsb_ctao.four_component import (
        HISTORICAL_FOUR_COMPONENT_MAE,
        run_four_component_benchmark,
    )
    if RECEIPT_PATH is None or not RECEIPT_PATH.is_file():
        raise FileNotFoundError(
            "Full mode requires NSB_BUILD_RECEIPT from ./ctao/install_nsb.sh / reproduce.sh"
        )
    FOUR = run_four_component_benchmark(
        config_path=ROOT / "ctao" / "configs" / "patat-fors1-spectral-v1.json",
        data_dir=RESULTS / "patat_data",
        output_dir=RESULTS / "four_component",
        solar_path=ROOT / "ctao" / "configs" / "patat-airglow-f107-observed-v1.json",
        nsb_receipt=RECEIPT_PATH,
        expected_nsb_commit=None,
    )
    adm = FOUR["admission"]
    print(f"Selected {adm['admitted_observations']} / {adm['admitted_nights']} nights")
    print(f"Evaluation {adm['evaluation_observations']} / {adm['evaluation_nights']} nights")
    print(f"NSB commit {adm['nsb'].get('git_commit')}")
    print(f"nsb2 {adm['nsb2_version']} @ {adm['nsb2_commit'][:12]}")
    print()
    print(f"{'configuration':28s} {'MAE_rel':>10s} {'RMSE':>10s}  CI95_rel")
    for row in FOUR["primary_table"]:
        ci = row["relative_mae_ci95"]
        ci_s = "—" if ci is None else f"[{ci[0]:.4f}, {ci[1]:.4f}]"
        print(f"{row['configuration']:28s} {row['mean_absolute_relative_error']:10.5f} "
              f"{row['rmse']:10.5f}  {ci_s}")
    print()
    print("Delta vs historical reference (this − reference):")
    for k, v in FOUR["metrics"]["historical_mae_delta_this_minus_reference"].items():
        print(f"  {k:28s} {v:+.3e}")
    print()
    print("Paired focus:")
    print(json.dumps(FOUR["metrics"]["paired_focus"], indent=2))

    from IPython.display import Image
    import shutil
    for src in sorted((RESULTS / "four_component" / "figures").glob("*.png")):
        shutil.copy2(src, FIGDIR / src.name)
        display(Markdown(f"**{src.name}**"))
        display(Image(filename=str(src)))
else:
    display(Markdown(
        "**Lightweight mode:** four-component Patat benchmark not executed. "
        "Do not inherit full-mode accuracy conclusions. Run `./ctao/reproduce.sh full`."
    ))
"""
    ),
    md(
        """### Historical reference check

Frozen development scores (prior NSB/Oxygen analyses; **not** this cell’s outputs):

| Configuration | Relative MAE |
|---|---:|
| NSB continuum, native | 0.36103 |
| NSB Oxygen, native | 0.23882 |
| NSB continuum, measured F10.7 | 0.35027 |
| NSB Oxygen, measured F10.7 | 0.16573 |
| nsb2, matched F10.7 | 0.17425 |
| Median baseline | 0.15245 |

In full mode, the deltas printed above should be ~machine precision if the v0.2.0 candidate preserves the frozen operator. Larger deltas require investigation (model change, selection, units, or bug)—not silent retuning.
"""
    ),
    md(
        """## 5 · Why Airglow-only vs total sky is not primary

An Airglow-only integral cannot match the measured total radiance that also contains zodiacal light and starlight. Large “Airglow-only MAE” values are expected and **must not** be quoted as full-model accuracy.

Full mode may optionally record a demoted airglow-only diagnostic; it never replaces §4.
"""
    ),
    code(
        r"""
AIRGLOW_ONLY = None
if MODE == "full" and os.environ.get("NSB_CTAO_RUN_AIRGLOW_ONLY", "0") == "1":
    from nsb_ctao.patat_oxygen import run_patat_oxygen_comparison
    AIRGLOW_ONLY = run_patat_oxygen_comparison(
        config_path=ROOT / "ctao" / "configs" / "patat-fors1-spectral-v1.json",
        data_dir=RESULTS / "patat_data",
        output_dir=RESULTS / "airglow_only_supplement",
        f107_sfu=None,
        include_nsb2=True,
        seed=SEED,
    )
    print("SUPPLEMENT ONLY — airglow-only vs total observed")
    for name, sc in AIRGLOW_ONLY["scores"].items():
        if isinstance(sc, dict) and "mae_relative" in sc:
            print(f"  {name}: MAE_rel={sc['mae_relative']:.5f}")
else:
    print("Airglow-only supplement skipped "
          "(set NSB_CTAO_RUN_AIRGLOW_ONLY=1 in full mode to enable).")
"""
    ),
    md(
        """## 6 · Independent OI photometer gate

Abastumani IQSY OI intensities (Rayleighs) — methodology / order-of-magnitude only.
Absolute admission remains **REJECTED**. LVM protected cohort sealed.
"""
    ),
    code(
        r"""
from nsb_ctao.oi_photometer import assess_abastumani_equivalence
from nsb_ctao.rayleigh import photons_per_ns_sr_from_rayleigh

OI_GATE = assess_abastumani_equivalence(
    ROOT / "ctao" / "assets" / "abas5577.int",
    ROOT / "ctao" / "assets" / "abas6300.int",
)
write_json(RESULTS / "oi_equivalence_gate.json", OI_GATE)
print("Admission:", OI_GATE["admission"])
for k, v in OI_GATE["gates"].items():
    print(f"  {k}: {v}")
"""
    ),
    md(
        """## 7 · PALACE Oxygen impact (within the full-model experiment)

Oxygen is exercised as the real NSB airglow selection (`PARANAL_PALACE_OXYGEN_V1`), not an external patch. On the Patat development evaluation set (full mode), compare continuum vs Oxygen under matched F10.7 using §4 paired statistics.

This does **not** authorize default promotion.
"""
    ),
    code(
        r"""
import nsb
import siderust
from datetime import datetime

coords = {"lon_deg": -70.404, "lat_deg": -24.627, "height_m": 2150.0}
observer = siderust.Observer(**coords)
night = datetime(2024, 1, 15, 2, 0, tzinfo=UTC)
from nsb_ctao.physical_checks import near_zenith_direction
target = near_zenith_direction(observer, night)
query = nsb.PointQuery(observer, night, target, components=nsb.ComponentMask.AIRGLOW)
edges = np.arange(550.0, 645.0, 1.0)
centers = 0.5 * (edges[:-1] + edges[1:])

def dens(model):
    cfg = (
        nsb.NsbModelConfig.generic_clear_sky()
        .with_site_profile(nsb.SiteProfile.CTA_SOUTH)
        .with_airglow_model(model)
        .with_solar_radio_flux_sfu(130.0)
    )
    ev = nsb.NsbEvaluator(cfg)
    return np.array([
        float(ev.integrate_band(query, float(lo), float(hi))) / (hi - lo)
        for lo, hi in zip(edges[:-1], edges[1:])
    ])

d_c = dens(nsb.AirglowModel.PARANAL_PALACE_V1)
d_o = dens(nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1)
fig, axes = plt.subplots(2, 1, figsize=(9, 6.0), sharex=True)
axes[0].step(centers, d_c, where="mid", label="Continuum", color="#5b7c99")
axes[0].step(centers, d_o, where="mid", label="Oxygen", color="#c47b4a")
axes[0].set_ylabel(r"$I_\gamma$ density")
axes[0].set_title("CTA-South airglow model spectrum (F10.7=130)")
axes[0].legend(); axes[0].grid(True, alpha=0.3)
axes[1].step(centers, d_o - d_c, where="mid", color="#2f6f4e")
axes[1].axhline(0, color="k", lw=0.8)
for lam in (557.9, 630.2, 636.6):
    axes[0].axvline(lam, color="0.5", ls=":", lw=1)
    axes[1].axvline(lam, color="0.5", ls=":", lw=1)
axes[1].set_xlabel("nm"); axes[1].set_ylabel("Oxygen − continuum")
axes[1].grid(True, alpha=0.3)
fig.tight_layout()
fig.savefig(FIGDIR / "05_oxygen_line_closeup.png", dpi=150)
plt.show()

if FOUR is not None:
    print("Full-model Oxygen vs continuum (matched F10.7), evaluation set:")
    print(json.dumps(FOUR["metrics"]["paired_focus"]["NSB_oxygen_f27_minus_NSB_continuum_f27"], indent=2))
"""
    ),
    md(
        r"""## 8 · CTAO-oriented scenarios (illustrative)

$$
R_{\mathrm{pe}} =
\Omega_{\mathrm{pixel}}
\int I_{\gamma}(\lambda)\, A_{\mathrm{opt}}(\lambda)\, \mathrm{PDE}(\lambda)\, d\lambda
$$

**A** = NSB spectral predictions · **B** = toy response rates · **C** = real CTAO camera validation (not claimed here).
"""
    ),
    code(
        r"""
from nsb_ctao.ctao_scenarios import run_ctao_scenarios

CTAO = run_ctao_scenarios(f107_values=(70.0, 130.0, 200.0))
write_json(RESULTS / "ctao_scenarios.json", CTAO)
print(CTAO["label"])
for row in CTAO["oxygen_effect_f107_130"]:
    print(f"  {row['site']}: Δband={100*row['rel_band']:.1f}%, ΔR_pe={100*row['rel_R_pe']:.1f}%")

fig, axes = plt.subplots(1, 2, figsize=(11, 4.2))
for ax, site in zip(axes, ["CTA_SOUTH", "CTA_NORTH"]):
    for model, color in (("continuum", "#5b7c99"), ("oxygen", "#c47b4a")):
        xs = [r["f107_sfu"] for r in CTAO["rows"] if r["site"] == site and r["airglow_model"] == model]
        ys = [r["R_pe_ns"] for r in CTAO["rows"] if r["site"] == site and r["airglow_model"] == model]
        ax.plot(xs, ys, "o-", color=color, label=model)
    ax.set_title(site); ax.set_xlabel("F10.7 [sfu]")
    ax.set_ylabel(r"$R_{pe}$ [pe/ns] (illustrative)")
    ax.grid(True, alpha=0.3); ax.legend()
fig.suptitle("Illustrative CTAO scenarios — not camera validation")
fig.tight_layout()
fig.savefig(FIGDIR / "07_ctao_rate_scenarios.png", dpi=150)
plt.show()
"""
    ),
    md("## 9 · Executive summary and conclusions"),
    code(
        r"""
from nsb_ctao.conclusions import build_conclusions

CONCLUSIONS = build_conclusions(
    physical=PHYSICAL,
    oi_gate=OI_GATE,
    patat=None,  # airglow-only supplement intentionally unused for primary claims
    ctao=CTAO,
    nsb_version=(MANIFEST.get("nsb") or {}).get("version"),
    four_component=FOUR,
)
write_json(RESULTS / "conclusions.json", {"schema": "nsb-ctao-conclusions-v1", "rows": CONCLUSIONS})

# Executive summary table from this run
if FOUR is not None:
    print("=== PRIMARY FULL-MODEL RESULTS (evaluation 94/17) ===")
    for row in FOUR["primary_table"]:
        print(f"  {row['configuration']:28s}  relMAE={row['mean_absolute_relative_error']:.5f}")
    print("Scientific status:", FOUR["admission"]["independence_status"])
else:
    print("=== PRIMARY FULL-MODEL RESULTS: not executed (lightweight) ===")

print()
for row in CONCLUSIONS:
    print("-" * 72)
    print("Q:", row["question"])
    print("  admission:", row["admission_status"])
    print("  conclusion:", row["scientific_conclusion"])

labels = [
    "Physical consistency",
    "Full 4-comp Patat (dev.)",
    "Independent OI absolute",
    "Oxygen → default",
    "CTAO camera validation",
]
status = [
    "PASS" if PHYSICAL["all_passed"] else "FAIL",
    "DEV" if FOUR is not None else "SKIP",
    "FAIL",
    "NO-GO",
    "ILLUSTRATIVE",
]
colors = {"PASS": "#2f6f4e", "DEV": "#9a7b2f", "FAIL": "#a33b2b", "NO-GO": "#a33b2b",
          "ILLUSTRATIVE": "#9a7b2f", "SKIP": "#888888"}
fig, ax = plt.subplots(figsize=(8.5, 3.6))
y = np.arange(len(labels))
ax.barh(y, [1]*len(labels), color=[colors[s] for s in status])
ax.set_yticks(y)
ax.set_yticklabels([f"{l} — {s}" for l, s in zip(labels, status)])
ax.set_xticks([]); ax.set_title("Evidence summary (this run)")
fig.tight_layout()
fig.savefig(FIGDIR / "08_evidence_summary.png", dpi=150)
plt.show()

print("\\n1. Physically consistent?", PHYSICAL["all_passed"])
print("2. Full-model Patat executed?", FOUR is not None)
print("3. Independent absolute Oxygen?", OI_GATE["admission"])
print("4. Oxygen ready for default?", "NO-GO")
print("5. CTAO camera validation?", "No — scenarios only")
"""
    ),
    md(
        """### Reproduction

```bash
./ctao/reproduce.sh lightweight
./ctao/reproduce.sh full
```

Artifacts: `ctao/results/<timestamp>/` including `four_component/` (full mode), `run_manifest.json`, and figures.

**Attribution:** NSB/PALACE (VPRamon), nsb2 (GerritRo et al.), Patat 2008 FORS1 (ESO), NOAA/NCEI IQSY, CTAO public site coordinates.
"""
    ),
]


def main() -> None:
    nb = {
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {
            "kernelspec": {
                "display_name": "Python 3",
                "language": "python",
                "name": "python3",
            },
            "language_info": {"name": "python", "pygments_lexer": "ipython3"},
        },
        "cells": CELLS,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(nb, indent=1) + "\n", encoding="utf-8")
    print(f"Wrote {OUT} ({len(CELLS)} cells)")


if __name__ == "__main__":
    main()
