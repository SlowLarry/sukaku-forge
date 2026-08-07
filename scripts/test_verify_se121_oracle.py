#!/usr/bin/env python3
"""Focused tests for the SE 1.2.1 differential verifier's corpus schema."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("verify-se121-oracle.py")
SPEC = importlib.util.spec_from_file_location("verify_se121_oracle", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


def corpus_document() -> dict:
    return {
        "schema_version": 2,
        "protected_cases_included": False,
        "format": "%r/%p/%d",
        "cases": [
            {
                "id": "schema-test",
                "puzzle": "." * 81,
                "expected_rating": "1.2/1.2/1.2",
            }
        ],
    }


class CorpusSchemaTests(unittest.TestCase):
    def test_schema_version_two_is_accepted(self) -> None:
        cases = VERIFIER.validate_corpus(corpus_document())
        self.assertEqual(len(cases), 1)
        self.assertEqual(cases[0]["id"], "schema-test")

    def test_missing_schema_version_is_rejected_clearly(self) -> None:
        document = corpus_document()
        del document["schema_version"]
        with self.assertRaisesRegex(
            VERIFIER.VerificationError,
            r"missing schema_version; expected 2",
        ):
            VERIFIER.validate_corpus(document)

    def test_unsupported_schema_versions_are_rejected_clearly(self) -> None:
        for unsupported in (1, 3, "2", 2.0, None):
            with self.subTest(schema_version=unsupported):
                document = corpus_document()
                document["schema_version"] = unsupported
                with self.assertRaisesRegex(
                    VERIFIER.VerificationError,
                    r"unsupported corpus schema_version .*; expected 2",
                ):
                    VERIFIER.validate_corpus(document)


if __name__ == "__main__":
    unittest.main()
