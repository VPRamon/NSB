"""Thin Python wrapper around the `nsb._native` PyO3 extension.

Build the native module with::

    maturin develop --features python

Then::

    import nsb
    r = nsb.calculate("CTAO-S", "2023-09-04 01:48:00", "SgrA*")
    print(r.integrated, r.v_mag)
"""
from . import _native  # noqa: F401

from ._native import (  # noqa: F401
    NsbComponent,
    NsbResult,
    calculate_py as calculate,
)

__all__ = ["NsbComponent", "NsbResult", "calculate"]
