#!/usr/bin/env python3
"""Render a full-sky Mollweide heatmap from a Starlight candidate-v5 HEALPix CSV."""

from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path

import healpy as hp
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.colors import LogNorm

DEFAULT_INPUT = Path("crates/nsb/data/starlight_nside128.csv")
DEFAULT_OUTPUT = Path(
    "docs/nsb_components/starlight/diagnostics/starlight_nside128_heatmap.png"
)

COMMENT_RE = re.compile(r"^#\s*([^=]+)=(.+)$")


def parse_header(path: Path) -> dict[str, str]:
    metadata: dict[str, str] = {}
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith("#"):
                break
            match = COMMENT_RE.match(line.strip())
            if match:
                metadata[match.group(1).strip()] = match.group(2).strip()
    required = ("nside", "ordering", "coordinate_frame")
    missing = [key for key in required if key not in metadata]
    if missing:
        raise ValueError(f"{path}: missing required map header fields: {missing}")
    return metadata


def load_quantity(
    path: Path,
    quantity: str,
) -> tuple[np.ndarray, dict[str, str]]:
    metadata = parse_header(path)
    nside = int(metadata["nside"])
    ordering = metadata["ordering"].lower()
    if ordering not in {"nested", "ring"}:
        raise ValueError(f"unsupported ordering {ordering!r}; expected nested or ring")

    npix = hp.nside2npix(nside)
    values = np.zeros(npix, dtype=np.float64)

    with path.open(encoding="utf-8", newline="") as handle:
        while True:
            position = handle.tell()
            line = handle.readline()
            if not line:
                raise ValueError(f"{path}: no data rows found")
            if not line.startswith("#"):
                handle.seek(position)
                break

        reader = csv.DictReader(handle)
        required_columns = {
            "flux": ("pixel", "flux_ph_m2_s"),
            "flux_per_admitted_source": ("pixel", "flux_ph_m2_s", "admitted_sources"),
            "admitted_sources": ("pixel", "admitted_sources"),
        }[quantity]

        if reader.fieldnames is None:
            raise ValueError(f"{path}: missing CSV column header")
        for column in required_columns:
            if column not in reader.fieldnames:
                raise ValueError(f"{path}: missing required column {column!r}")

        representation = metadata.get("representation", "dense").lower()
        sparse = representation == "sparse"
        occupied = np.zeros(npix, dtype=bool)

        seen = 0
        for row in reader:
            pixel = int(row["pixel"])
            if pixel < 0 or pixel >= npix:
                raise ValueError(f"{path}: pixel {pixel} out of range for nside={nside}")

            if quantity == "flux":
                value = float(row["flux_ph_m2_s"])
            elif quantity == "admitted_sources":
                value = float(row["admitted_sources"])
            else:
                admitted = int(row["admitted_sources"])
                flux = float(row["flux_ph_m2_s"])
                value = flux / admitted if admitted > 0 else 0.0

            values[pixel] = value
            occupied[pixel] = True
            seen += 1

    if not sparse and seen != npix:
        raise ValueError(f"{path}: expected {npix} occupied pixels, found {seen}")

    if sparse:
        values = values.astype(np.float64)
        values[~occupied] = hp.UNSEEN

    nest = ordering == "nested"
    # Pixel indices in the CSV are already in the declared ordering; do not reorder.
    return values, {**metadata, "nest": str(nest).lower(), "quantity": quantity}


def coordinate_frame_to_healpy(frame: str) -> str:
    mapping = {
        "galactic": "G",
        "equatorial": "C",
        "ecliptic": "E",
    }
    try:
        return mapping[frame.lower()]
    except KeyError as error:
        raise ValueError(
            f"unsupported coordinate_frame {frame!r}; expected one of {sorted(mapping)}"
        ) from error


def render_heatmap(
    values: np.ndarray,
    metadata: dict[str, str],
    output: Path,
    *,
    title: str | None,
    dpi: int,
    cmap: str,
) -> None:
    nest = metadata["nest"] == "true"
    coord = coordinate_frame_to_healpy(metadata["coordinate_frame"])
    quantity = metadata["quantity"]

    positive = values[values > 0]
    if positive.size == 0:
        raise ValueError("map has no positive values to display")

    vmin = float(np.min(positive))
    vmax = float(np.max(values))

    unit_labels = {
        "flux": "ph m⁻² s⁻¹",
        "flux_per_admitted_source": "ph m⁻² s⁻¹ per admitted source",
        "admitted_sources": "admitted sources",
    }
    quantity_labels = {
        "flux": "integrated pixel flux",
        "flux_per_admitted_source": "flux / admitted sources",
        "admitted_sources": "admitted source count",
    }

    fig = plt.figure(figsize=(12, 6))
    if quantity == "admitted_sources":
        hp.mollview(
            values,
            nest=nest,
            coord=coord,
            cmap=cmap,
            min=vmin,
            max=vmax,
            title=title
            or f"Starlight nside={metadata['nside']} — {quantity_labels[quantity]}",
            unit=unit_labels[quantity],
            fig=fig.number,
            hold=True,
        )
    else:
        hp.mollview(
            values,
            nest=nest,
            coord=coord,
            cmap=cmap,
            norm=LogNorm(vmin=vmin, vmax=vmax),
            title=title
            or f"Starlight nside={metadata['nside']} — {quantity_labels[quantity]}",
            unit=unit_labels[quantity],
            fig=fig.number,
            hold=True,
        )

    # Graticule helps orient Galactic features on the Mollweide projection.
    hp.graticule(dpar=30, dmer=30, coord=coord, color="0.6", alpha=0.35)

    output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output, dpi=dpi, facecolor="white")
    plt.close(fig)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Generate a Mollweide heatmap PNG from a Starlight candidate-v5 CSV."
    )
    parser.add_argument(
        "--input",
        type=Path,
        default=DEFAULT_INPUT,
        help=f"Input candidate map CSV (default: {DEFAULT_INPUT})",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help=f"Output PNG path (default: {DEFAULT_OUTPUT})",
    )
    parser.add_argument(
        "--quantity",
        choices=("flux", "flux_per_admitted_source", "admitted_sources"),
        default="flux_per_admitted_source",
        help="Scalar field to colour (default: flux_per_admitted_source)",
    )
    parser.add_argument(
        "--title",
        default=None,
        help="Optional plot title",
    )
    parser.add_argument(
        "--cmap",
        default="viridis",
        help="Matplotlib colormap name (default: viridis)",
    )
    parser.add_argument(
        "--dpi",
        type=int,
        default=150,
        help="Output image DPI (default: 150)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    input_path = args.input.resolve()
    output_path = args.output.resolve()

    if not input_path.is_file():
        print(f"error: input file not found: {input_path}", file=sys.stderr)
        return 1

    values, metadata = load_quantity(input_path, args.quantity)
    render_heatmap(
        values,
        metadata,
        output_path,
        title=args.title,
        dpi=args.dpi,
        cmap=args.cmap,
    )
    print(f"wrote {output_path} ({args.quantity}, nside={metadata['nside']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
