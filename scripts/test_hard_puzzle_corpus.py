#!/usr/bin/env python3
"""Integrity tests for the retained public hard-puzzle corpus."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import unittest


CORPUS = Path(__file__).with_name("hard-puzzle-corpus.json")
RATING_PATTERN = re.compile(r"[0-9]+\.[0-9]/[0-9]+\.[0-9]/[0-9]+\.[0-9]")
SOURCE_URL = (
    "https://docs.google.com/spreadsheets/d/"
    "1t-PsJT-pKGQEWjSbbNBXzLcxb5Inmooszntu9ZVCW_M/edit?gid=0#gid=0"
)


class HardPuzzleCorpusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.document = json.loads(CORPUS.read_text(encoding="utf-8"))
        cls.cases = cls.document["cases"]

    def test_schema_source_and_snapshot_are_pinned(self) -> None:
        self.assertEqual(self.document["schema_version"], 1)
        self.assertEqual(self.document["source_url"], SOURCE_URL)
        self.assertEqual(self.document["source_gid"], 0)
        self.assertEqual(self.document["source_license"], "unspecified")
        self.assertEqual(self.document["snapshot_date"], "2026-08-07")
        self.assertEqual(self.document["rating_format"], "ER/EP/ED")
        self.assertRegex(self.document["source_csv_sha256"], r"^[0-9a-f]{64}$")

    def test_all_972_ordered_cases_are_well_formed_and_unique(self) -> None:
        self.assertEqual(len(self.cases), 972)
        puzzles = []
        for case in self.cases:
            self.assertEqual(set(case), {"expanded_minlex", "rating"})
            puzzle = case["expanded_minlex"]
            puzzles.append(puzzle)
            self.assertEqual(len(puzzle), 81)
            self.assertLessEqual(set(puzzle), set(".123456789"))
            self.assertIsNotNone(RATING_PATTERN.fullmatch(case["rating"]))
        self.assertEqual(len(puzzles), len(set(puzzles)))

    def test_normalized_case_digest_and_endpoints_are_stable(self) -> None:
        normalized = "".join(
            f"{case['expanded_minlex']}\t{case['rating']}\n" for case in self.cases
        ).encode("ascii")
        self.assertEqual(
            hashlib.sha256(normalized).hexdigest(),
            self.document["cases_sha256"],
        )
        self.assertEqual(self.cases[0]["rating"], "11.9/11.9/3.4")
        self.assertEqual(self.cases[-1]["rating"], "10.4/2.0/2.0")


if __name__ == "__main__":
    unittest.main()
