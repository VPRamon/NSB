#!/usr/bin/env python3
"""Render an NSB Starlight HEALPix candidate as a Galactic heatmap.

The parser intentionally uses only the Python standard library so its input
contract can be tested in CI without visualization dependencies. Rendering
requires numpy, matplotlib, and healpy.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import math
from dataclasses import dataclass
from pathlib import Path

EXPECTED_ORDERING = "nested"
EXPECTED_FLUX_QUANTITY = "integrated_per_pixel"
EXPECTED_FLUX_UNIT = "ph_m-2_s-1"


@dataclass(frozen=True)
class StarlightMapData:
    """Validated data needed to render a sparse Starlight candidate."""

    nside: int
    metadata: dict[str, str]
    pixels: tuple[tuple[int, float], ...]


def sha256(path: Path) -> str:
    """Return the SHA-256 digest of a file."""

    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _parse_metadata(line: str) -> tuple[str, str] | None:
    body = line[1:].strip()
    if "=" not in body:
        return None
    key, value = body.split("=", 1)
    key = key.strip()
    value = value.strip()
    if not key:
        return None
    return key, value


def _require_metadata(metadata: dict[str, str], key: str, expected: str) -> None:
    actual = metadata.get(key)
    if actual != expected:
        raise ValueError(
            f"expected metadata {key}={expected!r}, found {actual!r}"
        )


def load_starlight_map(path: Path) -> StarlightMapData:
    """Load and validate the sparse candidate-map fields used by the plot."""

    metadata: dict[str, str] = {}
    header: list[str] | None = None
    pixel_index: int | None = None
    flux_index: int | None = None
    pixels: list[tuple[int, float]] = []
    previous_pixel = -1

    with path.open("r", encoding="utf-8", newline="") as stream:
        for line_number, line in enumerate(stream, start=1):
            stripped = line.strip()
            if not stripped:
                continue
            if stripped.startswith("#"):
                parsed = _parse_metadata(stripped)
                if parsed is not None:
                    key, value = parsed
                    metadata[key] = value
                continue

            fields = next(csv.reader([line]))
            if header is None:
                header = [field.strip() for field in fields]
                try:
                    pixel_index = header.index("pixel")
                    flux_index = header.index("flux_ph_m2_s")
                except ValueError as exc:
                    raise ValueError(
                        f"{path}:{line_number}: expected pixel and flux_ph_m2_s columns"
                    ) from exc
                continue

            if len(fields) != len(header):
                raise ValueError(
                    f"{path}:{line_number}: expected {len(header)} columns, "
                    f"found {len(fields)}"
                )

            assert pixel_index is not None
            assert flux_index is not None
            try:
                pixel = int(fields[pixel_index])
                flux = float(fields[flux_index])
            except ValueError as exc:
                raise ValueError(
                    f"{path}:{line_number}: invalid pixel or flux_ph_m2_s value"
                ) from exc

            if pixel <= previous_pixel:
                raise ValueError(
                    f"{path}:{line_number}: pixels must be strictly increasing"
                )
            if not math.isfinite(flux) or flux < 0.0:
                raise ValueError(
                    f"{path}:{line_number}: flux_ph_m2_s must be finite and non-negative"
                )

            pixels.append((pixel, flux))
            previous_pixel = pixel

    if header is None:
        raise ValueError(f"{path}: missing CSV header")
    if not pixels:
        raise ValueError(f"{path}: map has no occupied pixels")

    try:
        nside = int(metadata["nside"])
    except (KeyError, ValueError) as exc:
        raise ValueError(f"{path}: missing or invalid # nside metadata") from exc
    if nside <= 0 or nside & (nside - 1):
        raise ValueError(f"{path}: nside must be a positive power of two")

    _require_metadata(metadata, "ordering", EXPECTED_ORDERING)
    _require_metadata(metadata, "flux_quantity", EXPECTED_FLUX_QUANTITY)
    _require_metadata(metadata, "flux_unit", EXPECTED_FLUX_UNIT)

    pixel_count = 12 * nside * nside
    if pixels[-1][0] >= pixel_count:
        raise ValueError(
            f"{path}: pixel {pixels[-1][0]} is outside nside={nside} "
            f"domain [0, {pixel_count})"
        )

    return StarlightMapData(
        nside=nside,
        metadata=metadata,
        pixels=tuple(pixels),
    )


def default_title(path: Path, data: StarlightMapData) -> str:
    """Build the compact title used by the reference heatmap."""

    digest = sha256(path)
    return (
        f"NSB Starlight nside={data.nside} — integrated flux "
        f"(Galactic, {digest[:8]}…)"
    )


def _visualization_modules():
    try:
        import healpy as hp
        import matplotlib.pyplot as plt
        import numpy as np
    except ModuleNotFoundError as exc:
        raise SystemExit(
            "rendering requires numpy, matplotlib, and healpy; install them with "
            "'python -m pip install numpy matplotlib healpy'"
        ) from exc
    return hp, plt, np


def render_heatmap(
    path: Path,
    data: StarlightMapData,
    output: Path,
    *,
    norm: str,
    xsize: int,
    dpi: int,
    title: str | None,
) -> None:
    """Render a Galactic Mollweide heatmap from the sparse NESTED map."""

    hp, plt, np = _visualization_modules()

    pixel_count = 12 * data.nside * data.nside
    flux = np.zeros(pixel_count, dtype=float)
    for pixel, value in data.pixels:
        flux[pixel] = value

    positive = flux[flux > 0.0]
    if positive.size == 0:
        raise ValueError(f"{path}: map contains no positive flux")

    vmin = float(positive.min()) if norm == "log" else float(flux.min())
    vmax = float(flux.max())
    if not math.isfinite(vmax) or vmax <= 0.0:
        raise ValueError(f"{path}: map maximum flux must be finite and positive")

    plot_map = hp.ma(flux)
    if norm == "log":
        # HEALPix omissions are physical zero-flux pixels. Mask them for LogNorm
        # and paint masked values with the lowest colormap value below.
        plot_map.mask = flux <= 0.0

    cmap = plt.get_cmap("viridis").copy()
    cmap.set_bad(cmap(0.0))

    figure = plt.figure(figsize=(14.5, 7.5))
    hp.mollview(
        plot_map,
        fig=figure.number,
        nest=True,
        coord="G",
        xsize=xsize,
        cmap=cmap,
        norm=norm,
        min=vmin,
        max=vmax,
        cbar=False,
        notext=True,
        bgcolor="0.55",
        title=title or default_title(path, data),
    )
    hp.graticule(dpar=90.0, dmer=180.0, alpha=0.18, linewidth=0.8)

    sky_axes = figure.axes[0]
    sky_axes.text(
        0.96,
        0.05,
        "Galactic",
        transform=sky_axes.transAxes,
        ha="right",
        va="bottom",
        fontsize=12,
        fontweight="bold",
    )

    image = sky_axes.get_images()[0]
    colorbar = figure.colorbar(
        image,
        ax=sky_axes,
        orientation="horizontal",
        pad=0.07,
        shrink=0.60,
        aspect=32,
    )
    colorbar.set_label("ph m⁻² s⁻¹", fontsize=12)
    ticks = [vmin] if math.isclose(vmin, vmax) else [vmin, vmax]
    colorbar.set_ticks(ticks)
    colorbar.set_ticklabels([f"{value:.6g}" for value in ticks])

    output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(output, dpi=dpi, bbox_inches="tight")
    plt.close(figure)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Render the sparse NSB Starlight candidate as a Galactic Mollweide heatmap."
        )
    )
    parser.add_argument(
        "map",
        nargs="?",
        type=Path,
        default=Path("crates/nsb/data/starlight_nside128.csv"),
        help="candidate CSV (default: crates/nsb/data/starlight_nside128.csv)",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=Path("starlight_heatmap.png"),
        help="output PNG path (default: starlight_heatmap.png)",
    )
    parser.add_argument(
        "--norm",
        choices=("log", "linear"),
        default="log",
        help="color normalization (default: log)",
    )
    parser.add_argument(
        "--xsize",
        type=int,
        default=2000,
        help="Mollweide raster width in pixels (default: 2000)",
    )
    parser.add_argument(
        "--dpi",
        type=int,
        default=160,
        help="saved figure DPI (default: 160)",
    )
    parser.add_argument(
        "--title",
        help="override the automatic title",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.xsize <= 0:
        raise SystemExit("--xsize must be positive")
    if args.dpi <= 0:
        raise SystemExit("--dpi must be positive")
    if not args.map.is_file():
        raise SystemExit(f"candidate map does not exist: {args.map}")

    data = load_starlight_map(args.map)
    render_heatmap(
        args.map,
        data,
        args.output,
        norm=args.norm,
        xsize=args.xsize,
        dpi=args.dpi,
        title=args.title,
    )

    digest = sha256(args.map)
    pixel_count = 12 * data.nside * data.nside
    positive_flux = [flux for _, flux in data.pixels if flux > 0.0]
    print(
        f"wrote {args.output} "
        f"(nside={data.nside}, occupied={len(data.pixels)}/{pixel_count}, "
        f"positive_min={min(positive_flux):.6g}, "
        f"max={max(positive_flux):.6g}, sha256={digest})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
