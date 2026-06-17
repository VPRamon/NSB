#!/usr/bin/env python3
"""Skeleton generator for the NSB standard Galactic starlight map.

This script deliberately refuses to emit production data until a real
catalogue, photometric conversion, and provenance record are supplied.
"""

from __future__ import annotations

import argparse
import sys


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate data/starlight_galactic_map_v1.csv from a real catalogue."
    )
    parser.add_argument("--catalogue", help="Path to the source catalogue.")
    parser.add_argument("--output", default="data/starlight_galactic_map_v1.csv")
    parser.add_argument("--provenance", help="Path to provenance metadata.")
    parser.parse_args()

    print(
        "No production starlight map generator is implemented yet. "
        "Provide a real catalogue integration before emitting data.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
