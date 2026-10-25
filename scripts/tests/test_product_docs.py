# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Regression tests for the active-documentation hygiene gate."""

import copy
import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "check_product_docs", ROOT / "scripts/check-product-docs.py"
)
assert SPEC and SPEC.loader
docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(docs)


class PublicDocumentationTests(unittest.TestCase):
    def test_active_guides_are_checked(self):
        for path in (
            "README_PYPI.md",
            "crates/nsb-cli/README.md",
            "docs/developer-guide/public-api.md",
            "docs/specifications/validation-consumer.md",
            "docs/nsb_components/zodiacal/README.md",
        ):
            self.assertIn(path, docs.CURRENT_MARKDOWN)

    def test_development_narrative_is_rejected(self):
        path = ROOT / "docs" / "_synthetic-test-guide.md"
        issues = docs.markdown_issues(path, "See PR #123 for the tutorial.")
        self.assertTrue(any("development pull-request narrative" in issue for issue in issues))

    def test_bad_links_and_escaping_are_rejected(self):
        path = ROOT / "docs" / "_synthetic-test-guide.md"
        issues = docs.markdown_issues(path, "[absent](definitely-not-an-nsb-file.md)")
        self.assertTrue(any("missing local link" in issue for issue in issues))
        issues = docs.markdown_issues(path, "[escape](../../../../tmp/elsewhere.md)")
        self.assertTrue(any("link escapes repository" in issue for issue in issues))

    def test_external_and_valid_local_links(self):
        path = ROOT / "docs" / "_synthetic-test-guide.md"
        text = "[site](https://example.org/) [root](../README.md)"
        self.assertEqual(docs.markdown_issues(path, text), [])

    def test_notebook_hygiene_and_setup(self):
        path = ROOT / "examples" / "_synthetic-test.ipynb"
        valid = {"nbformat": 4, "cells": [{
            "cell_type": "code", "metadata": {"tags": ["setup"]},
            "source": ["REQUIREMENTS = ['nsb-py==0.2.0']\n"],
            "outputs": [], "execution_count": None,
        }]}
        self.assertEqual(docs.notebook_issues(path, valid), [])
        for field, value, message in (
            ("outputs", [{"output_type": "stream"}], "committed outputs"),
            ("execution_count", 1, "execution count"),
            ("source", ["def broken("], "invalid Python"),
            ("source", ["REQUIREMENTS = ['nsb-py']"], "select nsb-py==0.2.0"),
            ("source", ["import numpy\nREQUIREMENTS = ['nsb-py==0.2.0']"], "third-party"),
            ("metadata", {}, "first code cell"),
            ("source", None, "invalid source"),
        ):
            with self.subTest(field=field, value=value):
                nb = copy.deepcopy(valid)
                nb["cells"][0][field] = value
                self.assertTrue(any(message in issue for issue in docs.notebook_issues(path, nb)))
        self.assertTrue(docs.notebook_issues(path, {"nbformat": 3}))


if __name__ == "__main__":
    unittest.main()
