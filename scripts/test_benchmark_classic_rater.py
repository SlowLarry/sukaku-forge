#!/usr/bin/env python3
"""Policy and parser tests for benchmark-classic-rater.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).with_name("benchmark-classic-rater.py")
SPEC = importlib.util.spec_from_file_location("benchmark_classic_rater", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class EmbeddedCaseTests(unittest.TestCase):
    def test_cases_are_named_classic_grids_with_frozen_ratings(self) -> None:
        identifiers = set()
        protected = []
        for case in BENCHMARK.CASES:
            self.assertNotIn(case["id"], identifiers)
            identifiers.add(case["id"])
            self.assertEqual(len(case["puzzle"]), 81)
            self.assertTrue(set(case["puzzle"]) <= set(".0123456789"))
            self.assertIsNotNone(
                BENCHMARK.RATING_PATTERN.fullmatch(case["expected_rating"])
            )
            if case.get("major_milestone_only"):
                protected.append(case)
        self.assertEqual(len(protected), 1)
        self.assertEqual(protected[0]["id"], BENCHMARK.PROTECTED_CASE_ID)
        self.assertNotIn(BENCHMARK.PROTECTED_CASE_ID, BENCHMARK.DEFAULT_CASE_IDS)


class ProtectedPolicyTests(unittest.TestCase):
    def protected_arguments(self, *extra: str):
        return BENCHMARK.parse_arguments(
            [
                "--case",
                BENCHMARK.PROTECTED_CASE_ID,
                "--runs",
                "1",
                "--copies",
                "1",
                "--warmup",
                "0",
                *extra,
            ]
        )

    def test_exact_selection_authorization_and_one_shot_are_required(self) -> None:
        arguments = self.protected_arguments("--allow-major-milestone")
        cases = BENCHMARK.selected_cases(arguments.case_ids)
        BENCHMARK.require_benchmark_policy(arguments, cases)

        arguments = self.protected_arguments()
        with self.assertRaisesRegex(ValueError, "exact --case"):
            BENCHMARK.require_benchmark_policy(
                arguments, BENCHMARK.selected_cases(arguments.case_ids)
            )

        for option, value, message in (
            ("--runs", "2", "requires --runs 1"),
            ("--copies", "2", "requires --copies 1"),
            ("--warmup", "1", "requires --warmup 0"),
        ):
            arguments = BENCHMARK.parse_arguments(
                [
                    "--case",
                    BENCHMARK.PROTECTED_CASE_ID,
                    "--allow-major-milestone",
                    "--runs",
                    "1",
                    "--copies",
                    "1",
                    "--warmup",
                    "0",
                    option,
                    value,
                ]
            )
            with self.assertRaisesRegex(ValueError, message):
                BENCHMARK.require_benchmark_policy(
                    arguments, BENCHMARK.selected_cases(arguments.case_ids)
                )

    def test_protected_case_cannot_be_mixed_or_duplicated(self) -> None:
        for selected in (
            [BENCHMARK.PROTECTED_CASE_ID, "dynamic_forcing_chain_plus_9_3"],
            [BENCHMARK.PROTECTED_CASE_ID, BENCHMARK.PROTECTED_CASE_ID],
        ):
            arguments = BENCHMARK.parse_arguments(
                [
                    *sum((["--case", identifier] for identifier in selected), []),
                    "--allow-major-milestone",
                    "--runs",
                    "1",
                    "--copies",
                    "1",
                    "--warmup",
                    "0",
                ]
            )
            with self.assertRaisesRegex(ValueError, "exact --case"):
                BENCHMARK.require_benchmark_policy(
                    arguments, BENCHMARK.selected_cases(arguments.case_ids)
                )

    def test_authorization_is_rejected_for_an_ordinary_run(self) -> None:
        arguments = BENCHMARK.parse_arguments(["--allow-major-milestone"])
        with self.assertRaisesRegex(ValueError, "valid only"):
            BENCHMARK.require_benchmark_policy(
                arguments, BENCHMARK.selected_cases(arguments.case_ids)
            )


class OutputAndCommandTests(unittest.TestCase):
    def test_plain_and_generic_rust_ratings_are_parsed_strictly(self) -> None:
        self.assertEqual(
            BENCHMARK.parse_ratings("9.8/9.8/9.5\n", 1, "plain"),
            ["9.8/9.8/9.5"],
        )
        self.assertEqual(
            BENCHMARK.parse_ratings(
                "RESULT\t9.8\t9.8\t9.5\tER\tEP\tED\n", 1, "rust"
            ),
            ["9.8/9.8/9.5"],
        )
        with self.assertRaisesRegex(BENCHMARK.BenchmarkError, "unexpected"):
            BENCHMARK.parse_ratings("diagnostic\n9.8/9.8/9.5\n", 1, "bad")
        with self.assertRaisesRegex(BENCHMARK.BenchmarkError, "expected 2"):
            BENCHMARK.parse_ratings("9.8/9.8/9.5\n", 2, "short")

    def test_custom_commands_are_tokenized_without_a_shell(self) -> None:
        engine = BENCHMARK.custom_engine(
            "custom=python3 -c 'print(\"7.2/1.2/1.2\")'"
        )
        self.assertEqual(engine.label, "custom")
        self.assertEqual(
            engine.command_factory(Path("unused")),
            ["python3", "-c", 'print("7.2/1.2/1.2")'],
        )
        self.assertFalse(engine.pinned)
        self.assertFalse(engine.enforce_frozen_rating)

    def test_cpu_list_parser_handles_ranges(self) -> None:
        self.assertEqual(BENCHMARK.parse_cpu_list("0-2,5,7-8\n"), {0, 1, 2, 5, 7, 8})

    def test_timed_out_process_group_is_terminated(self) -> None:
        with self.assertRaises(BENCHMARK.BenchmarkTimeout) as caught:
            BENCHMARK.run_process(
                [sys.executable, "-c", "import time; time.sleep(10)"],
                "",
                0.02,
            )
        self.assertEqual(caught.exception.timeout, 0.02)
        self.assertLess(caught.exception.elapsed, 3.0)


if __name__ == "__main__":
    unittest.main()
