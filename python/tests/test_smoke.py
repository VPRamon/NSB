# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
import nsb


def test_import_and_versions_are_exposed():
    assert nsb.__version__
    assert nsb.MODEL_VERSION.startswith("nsb-model-")
    assert nsb.SIDERUST_VERSION == "0.12.0"
    assert nsb.SIDERUST_SOURCE == "crates.io:siderust:0.12.0"
