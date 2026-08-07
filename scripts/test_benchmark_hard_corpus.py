#!/usr/bin/env python3
"""Focused tests for benchmark-hard-corpus.py."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("benchmark-hard-corpus.py")
SPEC = importlib.util.spec_from_file_location("benchmark_hard_corpus", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class HardCorpusBenchmarkTests(unittest.TestCase):
    def setUp(self) -> None:
        self.document = BENCHMARK.load_corpus(BENCHMARK.DEFAULT_CORPUS)

    def test_default_slice_is_the_first_ten(self) -> None:
        arguments = BENCHMARK.parse_arguments([])
        self.assertEqual(
            arguments.rater,
            BENCHMARK.ROOT / "target" / "rater" / "sukaku-forge-rate",
        )
        selected = BENCHMARK.select_cases(
            self.document, arguments.start, arguments.limit
        )
        self.assertEqual(len(selected), 10)
        self.assertEqual(selected[0]["rating"], "11.9/11.9/3.4")
        self.assertEqual(selected[-1]["rating"], "11.8/11.8/3.4")

    def test_slice_bounds_are_strict(self) -> None:
        with self.assertRaisesRegex(ValueError, "positive"):
            BENCHMARK.select_cases(self.document, 0, 1)
        with self.assertRaisesRegex(ValueError, "corpus has 972"):
            BENCHMARK.select_cases(self.document, 970, 4)

    def test_rating_output_count_and_format_are_strict(self) -> None:
        self.assertEqual(
            BENCHMARK.parse_ratings("11.9/11.9/3.4\n11.8/11.8/3.4\n", 2),
            ["11.9/11.9/3.4", "11.8/11.8/3.4"],
        )
        with self.assertRaisesRegex(BENCHMARK.CorpusBenchmarkError, "expected 2"):
            BENCHMARK.parse_ratings("11.9/11.9/3.4\n", 2)
        with self.assertRaisesRegex(BENCHMARK.CorpusBenchmarkError, "malformed"):
            BENCHMARK.parse_ratings("not-a-rating\n", 1)

    def test_load_rejects_a_tampered_case_without_a_matching_digest(self) -> None:
        tampered = dict(self.document)
        tampered["cases"] = [dict(case) for case in self.document["cases"]]
        tampered["cases"][0]["rating"] = "1.0/1.0/1.0"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tampered.json"
            path.write_text(json.dumps(tampered), encoding="utf-8")
            with self.assertRaisesRegex(
                BENCHMARK.CorpusBenchmarkError, "digest mismatch"
            ):
                BENCHMARK.load_corpus(path)

    def test_load_rejects_a_non_object_document(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "array.json"
            path.write_text("[]", encoding="utf-8")
            with self.assertRaisesRegex(
                BENCHMARK.CorpusBenchmarkError, "root must be an object"
            ):
                BENCHMARK.load_corpus(path)


if __name__ == "__main__":
    unittest.main()
