# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Compact graphical report and closed artifact inventory."""

from __future__ import annotations

import html
import json
import platform
import subprocess
import uuid
from datetime import UTC, datetime
from importlib import metadata
from pathlib import Path

import markdown
import numpy as np

from .data import UNIT, sha256
from .models import COMPONENTS, MODELS

LIMITATIONS = """Development-only evidence: these Patat residuals influenced Oxygen development.
The 17 evaluation nights are not an untouched holdout. Oxygen remains experimental and
Automatic remains continuum-only. No site calibration or generalized superiority is claimed.
Missing non-oxygen line families and molecular absorption, PALACE solar extrapolation,
Starlight population differences (NSB map versus nsb2 G>=15), research-only Rieke solar
Starlight redistribution, and atmospheric propagation differences limit the comparison.
nsb2 source-level native zodiacal radiance has no added NSB extinction operator.
Single-site restricted dark-time conditions cannot establish geographic generalization.
FORS flux calibration has a few-percent common uncertainty; no per-spectrum statistical
uncertainty is published. Bootstrap intervals describe night sampling, not that systematic.
Observed air wavelengths and modeled vacuum bins retain the historical broadband convention;
the feature diagnostic is not an instrument-convolved full-sky spectrum."""

HISTORICAL = {
    "classification": "published development evidence; not a new execution",
    "benchmark_nsb_commit": "1b2556727205694c7765ef0d254c5d1b301582ec",
    "oxygen_numerical_commit": "168e73dae89060423eebc344871b0084d217c305",
    "oxygen_merge_commit": "57584ee298f25f8af7a6f169d71e87f8a59eea9a",
    "relative_mae": {
        "NSB Standard native": 0.36103,
        "NSB Oxygen native": 0.23882,
        "nsb2 native": 0.17087,
        "NSB Oxygen measured F10.7": 0.16573,
        "nsb2 matched F10.7": 0.17425,
        "baseline": 0.15245,
    },
    "matched_superiority": "Not established; paired CI includes zero",
    "solar_extrapolation": "79/103 outside PALACE 67–166 sfu",
    "public_evidence": "https://github.com/VPRamon/NSB/blob/57584ee298f25f8af7a6f169d71e87f8a59eea9a/docs/nsb_components/airglow/validation/oxygen-and-calibration-v1.md",
}


def json_write(path, value):
    Path(path).write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def new_run(root):
    path = Path(root) / (
        "run-" + datetime.now(UTC).strftime("%Y%m%dT%H%M%S") + "-" + uuid.uuid4().hex[:12]
    )
    path.mkdir(parents=True, exist_ok=False)
    return path


def code_identity():
    root = Path(__file__).resolve().parent
    files = {p.name: sha256(p) for p in sorted(root.glob("*")) if p.is_file()}

    for name in ("uv.lock", "pyproject.toml", "nsb-vs-nsb2.ipynb"):
        artifact = root.parents[1] / name
        if artifact.is_file():
            files[name] = sha256(artifact)

    def git(*args):
        try:
            return subprocess.check_output(
                ["git", "-C", str(root), *args], text=True, stderr=subprocess.DEVNULL
            ).strip()
        except (OSError, subprocess.CalledProcessError):
            return "unavailable (installed wheel); source file hashes recorded"

    return {
        "git_commit": git("rev-parse", "HEAD"),
        "git_status": git("status", "--porcelain"),
        "files": files,
    }


def seal(path, provenance):
    """Nonrecursive checksum manifest: detached SHA covers manifest, manifest covers payloads."""
    if (path / "manifest.json").exists():
        raise FileExistsError("Run already sealed")
    manifest = {
        **provenance,
        "created_utc": datetime.now(UTC).isoformat(),
        "validation_code": code_identity(),
        "python": platform.python_version(),
        "dependencies": {d.metadata["Name"]: d.version for d in metadata.distributions()},
        "artifacts": {
            p.relative_to(path).as_posix(): sha256(p)
            for p in sorted(path.rglob("*"))
            if p.is_file()
        },
    }
    json_write(path / "manifest.json", manifest)
    (path / "manifest.sha256").write_text(sha256(path / "manifest.json") + "  manifest.json\n")
    verify(path)


def verify(path):
    path = Path(path)
    if any(p.is_symlink() for p in (path, *path.parents)):
        raise ValueError("Symlink in report path")
    for p in path.rglob("*"):
        if p.is_symlink():
            raise ValueError("Symlink report artifact")
    if (path / "manifest.sha256").read_text() != sha256(
        path / "manifest.json"
    ) + "  manifest.json\n":
        raise ValueError("Manifest checksum mismatch")
    payload = json.loads((path / "manifest.json").read_text())
    names = payload["artifacts"]
    actual = {p.relative_to(path).as_posix() for p in path.rglob("*") if p.is_file()}
    if actual != set(names) | {"manifest.json", "manifest.sha256"}:
        raise ValueError("Report inventory mismatch")
    for name, digest in names.items():
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts or relative.as_posix() != name:
            raise ValueError("Unsafe manifest path")
        if sha256(path / relative) != digest:
            raise ValueError(f"Artifact checksum mismatch: {name}")
    return payload


def figures(path, rows, predictions, metrics, spectral):
    import matplotlib.pyplot as plt

    plt.rcParams.update(
        {
            "font.size": 10,
            "axes.spines.top": False,
            "axes.spines.right": False,
            "savefig.dpi": 180,
            "svg.fonttype": "none",
        }
    )
    directory = path / "figures"
    directory.mkdir()
    plots = []
    ids = [i for i, r in enumerate(rows) if r["split"] == "evaluation"]
    y = np.array([r["observed"] for r in rows])[ids]
    nights = np.array([r["night"] for r in rows])[ids]

    def save(fig, name, caption):
        fig.tight_layout()
        for suffix in ("png", "svg"):
            fig.savefig(directory / f"{name}.{suffix}")
        plt.close(fig)
        plots.append((name, caption))

    for scenario, model_rows in predictions.items():
        if scenario not in metrics:
            continue
        data = {
            key: np.array([r["total"] for r in values])[ids] for key, values in model_rows.items()
        }
        fig, ax = plt.subplots(figsize=(6, 5))
        for model in MODELS:
            ax.scatter(y, data[model], s=20, alpha=0.65, label=model)
        hi = max(y.max(), *(data[k].max() for k in MODELS)) * 1.05
        ax.plot([0, hi], [0, hi], color="black", lw=1)
        ax.set(
            xlabel=f"Observed [{UNIT}]",
            ylabel=f"Predicted [{UNIT}]",
            title=f"{scenario}: 365–650 nm",
        )
        ax.legend()
        save(
            fig,
            f"{scenario}-predicted-observed",
            "Evaluation spectra; ideal 1:1 line. Conditional research total includes Starlight proxy.",
        )
        fig, ax = plt.subplots(figsize=(7, 4))
        for j, (_key, value) in enumerate(metrics[scenario]["models"].items()):
            mean = value["mean_absolute_relative_error"]
            lo, hi = value["relative_mae_ci95"]
            ax.plot([j, j], [lo, hi], color=f"C{j}", lw=2)
            ax.scatter([j], [mean], color=f"C{j}")
        ax.set(
            xticks=range(4),
            xticklabels=list(metrics[scenario]["models"]),
            ylabel="Mean absolute relative error",
            title=f"{scenario}: 95% night-cluster intervals",
        )
        save(
            fig,
            f"{scenario}-model-errors",
            "Equal weight per spectrum; 2,000 night resamples, seed 182. Baseline fitted on calibration nights only.",
        )
        fig, ax = plt.subplots(figsize=(7, 4))
        residuals = {key: (data[key] - y) / y for key in MODELS}
        bins = np.linspace(
            min(v.min() for v in residuals.values()), max(v.max() for v in residuals.values()), 21
        )
        for key, value in residuals.items():
            ax.hist(value, bins=bins, histtype="step", lw=1.6, label=key)
        ax.axvline(0, color="black", lw=1)
        ax.set(
            xlabel="Signed relative residual (prediction − observation)/observation",
            ylabel="Spectra",
            title=scenario,
        )
        ax.legend()
        save(fig, f"{scenario}-residuals", "Common bins retain overprediction and underprediction.")
        fig, ax = plt.subplots(figsize=(8, 4))
        delta = abs(data["NSB Standard"] - y) / y - abs(data["NSB Oxygen"] - y) / y
        ax.bar(range(len(y)), delta, color=np.where(delta >= 0, "tab:blue", "tab:red"), width=1.0)
        ax.axhline(0, color="black", lw=1)
        ax.set(
            xlabel="Evaluation observation index (IDs in observations.json)",
            ylabel="Relative AE(Standard) − AE(Oxygen)",
            title=f"{scenario}: positive = improvement; negative = degradation",
        )
        save(
            fig,
            f"{scenario}-oxygen-improvement",
            "All evaluation spectra in frozen order; no favorable-case filtering.",
        )
        fig, ax = plt.subplots(figsize=(7, 4))
        bottom = np.zeros(3)
        for part in COMPONENTS:
            values = [np.mean([model_rows[k][i]["components"][part] for i in ids]) for k in MODELS]
            ax.bar(MODELS, values, bottom=bottom, label=part)
            bottom += values
        ax.set(
            ylabel=f"Mean contribution [{UNIT}]",
            title=f"{scenario}: conventions differ across models",
        )
        ax.legend(fontsize=8)
        save(
            fig,
            f"{scenario}-components",
            "Named components do not imply identical populations or atmospheric operators; nsb2 has combined SkyCalc Airglow.",
        )
        fig, ax = plt.subplots(figsize=(8, 4))
        unique = np.unique(nights)
        for key in MODELS:
            values = [np.mean(residuals[key][nights == n]) for n in unique]
            ax.plot(range(len(unique)), values, marker="o", label=key)
        ax.axhline(0, color="black", lw=1)
        ax.set(
            xticks=range(len(unique)),
            xticklabels=[str(n) for n in unique],
            ylabel="Mean signed relative residual",
            xlabel="Independent night (integer MJD)",
            title=scenario,
        )
        ax.tick_params(axis="x", rotation=70)
        ax.legend(fontsize=8)
        save(
            fig,
            f"{scenario}-nights",
            "Night-specific means reveal concentration of gains or losses.",
        )
    if metrics:
        fig, axes = plt.subplots(1, len(metrics), figsize=(7 * len(metrics), 4), squeeze=False)
        for ax, scenario in zip(axes[0], metrics, strict=True):
            x = np.array([rows[i].get("f27", np.nan) for i in ids])
            available = np.isfinite(x)
            if not available.any():
                ax.text(0.05, 0.5, "Verified measured F10.7 unavailable", transform=ax.transAxes)
            else:
                ax.axvspan(67, 166, color="green", alpha=0.12, label="PALACE training range")
                for key in MODELS:
                    p = np.array([r["total"] for r in predictions[scenario][key]])[ids]
                    ax.scatter(x, abs(p - y) / y, label=key, s=20, alpha=0.7)
                ax.set_xlim(min(60, float(x.min()) - 5), max(175, float(x.max()) + 5))
            ax.set(
                xlabel="Observed centered 27-day F10.7 [sfu]",
                ylabel="Absolute relative error",
                title=scenario,
            )
            ax.legend(fontsize=8)
        save(
            fig,
            "solar-sensitivity",
            "Native and matched inputs stay separate. Shaded domain is 67–166 sfu; all points outside are extrapolation.",
        )
    if spectral:
        fig, axes = plt.subplots(1, 2, figsize=(12, 4.8), sharey=True)
        for ax, limits in zip(axes, ((552, 563), (625, 642)), strict=True):
            ax.plot(
                spectral["air_wavelength_nm"],
                spectral["observed_total_density"],
                color="black",
                label="Observed TOTAL; air wavelength",
            )
            for name in ("standard", "oxygen"):
                ax.stairs(
                    spectral[name],
                    spectral["vacuum_bin_edges_nm"],
                    label=f"NSB AIRGLOW {name}; vacuum 1 nm bins",
                )
            ax.set(
                xlim=limits,
                xlabel="Wavelength [nm]",
                ylabel=r"Radiance density [ph cm$^{-2}$ ns$^{-1}$ sr$^{-1}$ nm$^{-1}$]",
            )
        axes[1].set_ylabel("")
        axes[0].legend(fontsize=7)
        fig.suptitle(
            f"{spectral['id']}\n{spectral['scenario']}: observed total (air); model Airglow (1 nm vacuum bins)",
            fontsize=10,
        )
        save(
            fig,
            "oxygen-features",
            "Air/vacuum coordinates are labeled separately. Observed total includes other components and blends; model curves contain only Airglow.",
        )
    return plots


def finish(path, *, config, rows, predictions, metrics, admission, provenance, spectral=None):
    plots = figures(path, rows, predictions, metrics, spectral) if metrics else []
    compact = [{k: v for k, v in r.items() if k not in ("wave", "energy")} for r in rows]
    payloads = {
        "config.snapshot.json": config,
        "observations.json": compact,
        "predictions.json": predictions,
        "metrics.json": metrics,
        "admission.json": admission,
        "historical-evidence.json": HISTORICAL,
        "spectral-diagnostic.json": spectral,
    }
    for name, value in payloads.items():
        json_write(path / name, value)
    text = f"# NSB / nsb2 observational experiment\n\n**{admission['classification']}**\n\n"
    text += LIMITATIONS + "\n\n"
    text += (
        "## Admission and availability\n\n```json\n" + json.dumps(admission, indent=2) + "\n```\n\n"
    )
    for scenario, value in metrics.items():
        text += f"## {scenario}\n\n| Model | N / nights | Bias | MAE | RMSE | Relative MAE | 95% CI |\n|---|---:|---:|---:|---:|---:|---|\n"
        for key, m in value["models"].items():
            text += f"| {key} | {m['n']} / {m['nights']} | {m['relative_bias']:.5f} | {m['mae']:.5f} | {m['rmse']:.5f} | {m['mean_absolute_relative_error']:.5f} | {m['relative_mae_ci95']} |\n"
        text += "\nPaired differences (left minus right; negative favors left):\n\n"
        text += "| Pair | Relative MAE difference | 95% night-cluster CI |\n|---|---:|---|\n"
        for key, m in value["paired"].items():
            text += f"| {key} | {m['relative_mae_difference']:.6f} | {m['relative_mae_ci95']} |\n"
        text += "\nIntervals crossing zero do not demonstrate superiority. Numerical gains do not grant scientific admission.\n\n"
    for name, caption in plots:
        text += f"![{name}](figures/{name}.png)\n\n{caption}\n\n"
    text += (
        "## Historical context (separate, rounded development evidence)\n\n"
        + json.dumps(HISTORICAL, indent=2)
        + "\n\n"
        "Historical evidence is displayed, never overwritten or called a byte-identical replay. "
        "See README.md for exact-identity reference checking and public-access limits.\n"
    )
    (path / "report.md").write_text(text)
    # Escape all data and include figures as real local images for a portable report.
    body = '<!doctype html><meta charset="utf-8"><title>NSB observational comparison</title>'
    body += "<style>body{max-width:1100px;margin:2em auto;font:16px system-ui;line-height:1.5}pre{white-space:pre-wrap;background:#f4f5f7;padding:1em}img{max-width:100%}table{border-collapse:collapse}td,th{padding:.5em;border-bottom:1px solid #ddd}</style>"
    body += markdown.markdown(html.escape(text, quote=False), extensions=["tables", "fenced_code"])
    (path / "report.html").write_text(body)
    seal(path, {**provenance, "config_sha256": sha256(path / "config.snapshot.json")})
    return path
