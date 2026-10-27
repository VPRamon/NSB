#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "api_migration", ROOT / "scripts/check-api-migration.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
MIGRATION = json.loads(
    (ROOT / "crates/nsb/api/target-direction-migration.json").read_text()
)
BEFORE = "\n".join(p["before"] for p in MIGRATION)
AFTER = "\n".join(p["after"] for p in MIGRATION)


class MigrationTests(unittest.TestCase):
    def test_exact_migration_and_ordinary_additions_pass(self):
        self.assertTrue(module.check(BEFORE, AFTER, MIGRATION))
        self.assertFalse(module.check(AFTER, AFTER + "\npub fn nsb::additive()", MIGRATION))

    def test_unrelated_signature_change_during_migration_fails(self):
        with self.assertRaisesRegex(ValueError, "unapproved"):
            module.check(BEFORE + "\npub fn nsb::unrelated()", AFTER, MIGRATION)

    def test_approximate_migration_is_not_authorized(self):
        with self.assertRaisesRegex(ValueError, "unapproved"):
            module.check(BEFORE, AFTER.replace("target: impl", "target: &impl"), MIGRATION)

    def test_subsequent_changes_to_migrated_items_fail(self):
        with self.assertRaisesRegex(ValueError, "unapproved"):
            module.check(AFTER, AFTER.replace("TargetDirection", "OtherTarget"), MIGRATION)


if __name__ == "__main__":
    unittest.main()
