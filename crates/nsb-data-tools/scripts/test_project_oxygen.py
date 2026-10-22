# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Opt-in source-byte reproduction: no network and no PALACE code execution."""
import os
from pathlib import Path

import pytest

from project_oxygen import project


def test_pinned_palace_source_reproduces_the_runtime_asset():
    archive = os.environ.get("NSB_PALACE_V1_ARCHIVE")
    if not archive:
        pytest.skip("Set NSB_PALACE_V1_ARCHIVE to checksum-pinned PALACE.zip")
    output = Path(__file__).resolve().parents[2] / "nsb/data/airglow_palace_oxygen_v1.json"
    assert project(Path(archive)) == output.read_text()
