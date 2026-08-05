#!/usr/bin/env python3
"""Focused static tests for the compact Java/Rust trace contract."""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("benchmark-java-rust.py")
SPEC = importlib.util.spec_from_file_location("benchmark_java_rust", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class CompactTraceTests(unittest.TestCase):
    def test_crlf_is_reduced_to_the_exact_version_one_contract(self) -> None:
        grid = "1" * 81
        candidates = "." * 729
        step = f"STEP\t9.0\tExample: r1c1.1 off\t{grid}\t{candidates}"
        result = "RESULT\t9.0\t2.0\t1.0\tExample\tHidden Single\tNaked Single"
        actual = BENCHMARK.parse_compact_trace(
            f"{step}\r\n{result}\r\n", "test trace"
        )
        canonical = f"{step}\n{result}\n".encode("utf-8")
        final_state = f"{grid}\n{candidates}\n".encode("utf-8")
        expected = {
            "version": 1,
            "step_count": 1,
            "final_grid": grid,
            "final_state_sha256": hashlib.sha256(final_state).hexdigest(),
            "sha256": hashlib.sha256(canonical).hexdigest(),
        }
        self.assertEqual(actual["contract"], expected)
        self.assertEqual(actual["rating"], "9.0/2.0/1.0")
        BENCHMARK.require_trace_contract("test trace", actual, expected)

    def test_malformed_or_reordered_records_are_rejected(self) -> None:
        result = "RESULT\t1.0\t1.0\t1.0\tA\tB\tC"
        malformed_step = "STEP\t1.0\tExample\tshort\tshort"
        with self.assertRaisesRegex(RuntimeError, "STEP state"):
            BENCHMARK.parse_compact_trace(
                f"{malformed_step}\n{result}\n", "malformed"
            )

        grid = "1" * 81
        candidates = "." * 729
        step = f"STEP\t1.0\tExample\t{grid}\t{candidates}"
        with self.assertRaisesRegex(RuntimeError, "STEP after RESULT"):
            BENCHMARK.parse_compact_trace(f"{result}\n{step}\n", "reordered")

    def test_only_the_four_normal_forum_cases_have_compact_contracts(self) -> None:
        document = json.loads(BENCHMARK.DEFAULT_CASES.read_text(encoding="utf-8"))
        contracted = {
            case["id"]: case
            for case in document["cases"]
            if "expected_trace" in case and not case.get("major_milestone_only", False)
        }
        expected_ids = {
            "forum_hard_9_6",
            "forum_hard_9_8",
            "forum_hard_10_4",
            "forum_hard_10_5_probe",
        }
        self.assertEqual(set(contracted), expected_ids)
        for case in contracted.values():
            self.assertEqual(case["original_expected_rating"], case["expected_rating"])
            self.assertEqual(case["expected_trace"]["version"], 1)
            self.assertEqual(len(case["expected_trace"]["final_grid"]), 81)
            self.assertEqual(len(case["expected_trace"]["final_state_sha256"]), 64)
            self.assertEqual(len(case["expected_trace"]["sha256"]), 64)
            self.assertFalse(case.get("major_milestone_only", False))

        protected = next(
            case
            for case in document["cases"]
            if case["id"] == "user_extreme_major_milestone_probe"
        )
        self.assertTrue(protected["major_milestone_only"])
        self.assertEqual(
            protected["expected_trace"],
            {
                "version": 1,
                "step_count": 133,
                "final_grid": (
                    "984721635635849127217365498852917364341256879769483251"
                    "123598746576134982498672513"
                ),
                "final_state_sha256": (
                    "56e2bc6c545ea82749276db2914ee2f6743bdd657795789db5f244115044f3fd"
                ),
                "sha256": (
                    "594d23c1329c5ad8c9a884a093a72d2b7c1d981936137ee050f034d01d4c2ef7"
                ),
            },
        )


class MajorMilestoneBenchmarkTests(unittest.TestCase):
    def setUp(self) -> None:
        document = json.loads(BENCHMARK.DEFAULT_CASES.read_text(encoding="utf-8"))
        self.case = next(
            case
            for case in document["cases"]
            if case["id"] == "user_extreme_major_milestone_probe"
        )

    def test_authorized_unfrozen_case_is_limited_to_one_solve_per_engine(self) -> None:
        unfrozen = {
            key: value
            for key, value in self.case.items()
            if key not in {"expected_rating", "original_expected_rating"}
        }
        copies = BENCHMARK.effective_copies(unfrozen, None)
        self.assertEqual(copies, 1)
        BENCHMARK.require_major_milestone_policy(unfrozen, True, True, 1, copies)

        with self.assertRaisesRegex(ValueError, "requires --runs 1"):
            BENCHMARK.require_major_milestone_policy(unfrozen, True, True, 2, 1)
        with self.assertRaisesRegex(ValueError, "effective --copies 1"):
            BENCHMARK.require_major_milestone_policy(unfrozen, True, True, 1, 2)

    def test_both_explicit_selection_and_authorization_are_required(self) -> None:
        with self.assertRaisesRegex(ValueError, "exact --case"):
            BENCHMARK.require_major_milestone_policy(self.case, False, True, 1, 1)
        with self.assertRaisesRegex(ValueError, "exact --case"):
            BENCHMARK.require_major_milestone_policy(self.case, True, False, 1, 1)

    def test_java_and_rust_rating_parsers(self) -> None:
        self.assertEqual(
            BENCHMARK.parse_java_rating(
                "Picked up JAVA_TOOL_OPTIONS\n11.8/1.2/1.2\n", "java"
            ),
            "11.8/1.2/1.2",
        )
        self.assertEqual(
            BENCHMARK.parse_rust_rating(
                "RESULT\t11.8\t1.2\t1.2\tER\tEP\tED\n", "rust"
            ),
            "11.8/1.2/1.2",
        )
        with self.assertRaisesRegex(RuntimeError, "malformed java rating"):
            BENCHMARK.parse_java_rating("not-a-rating\n", "java")
        with self.assertRaisesRegex(RuntimeError, "expected one RESULT"):
            BENCHMARK.parse_rust_rating("", "rust")

    def test_unfrozen_rating_requires_three_engine_consensus(self) -> None:
        ratings = {
            "java-original": "11.8/1.2/1.2",
            "java-optimized": "11.8/1.2/1.2",
            "rust-release": "11.8/1.2/1.2",
        }
        self.assertEqual(
            BENCHMARK.require_rating_consensus(self.case["id"], ratings),
            "11.8/1.2/1.2",
        )
        ratings["rust-release"] = "11.9/1.2/1.2"
        with self.assertRaisesRegex(RuntimeError, "rating disagreement"):
            BENCHMARK.require_rating_consensus(self.case["id"], ratings)

        del ratings["rust-release"]
        with self.assertRaisesRegex(RuntimeError, "incomplete unfrozen rating set"):
            BENCHMARK.require_rating_consensus(self.case["id"], ratings)

    def test_frozen_protected_case_stays_on_single_solve_path(self) -> None:
        self.assertEqual(self.case["expected_rating"], "11.8/1.2/1.2")
        self.assertEqual(self.case["original_expected_rating"], "11.8/1.2/1.2")
        self.assertTrue(BENCHMARK.uses_single_solve_rating_path(self.case))
        BENCHMARK.require_frozen_major_milestone_rating(
            self.case, "11.8/1.2/1.2"
        )
        with self.assertRaisesRegex(RuntimeError, "major-milestone rating changed"):
            BENCHMARK.require_frozen_major_milestone_rating(
                self.case, "11.9/1.2/1.2"
            )


class PGExplainerBenchmarkTests(unittest.TestCase):
    def test_command_uses_only_pg_supported_options(self) -> None:
        command = BENCHMARK.pg_command(
            "java", Path("PGExplainer.jar"), "sudoku.serate", "%r/%p/%d"
        )
        self.assertEqual(
            command,
            [
                "java",
                "-Xrs",
                "-Xmx500m",
                "-cp",
                "PGExplainer.jar",
                "sudoku.serate",
                "--format=%r/%p/%d",
                "--input=-",
            ],
        )
        self.assertNotIn("--threads=1", command)

    def test_only_classic_cases_are_supported(self) -> None:
        classic = {"copies": 6, "puzzle": "0" * 81}
        variant = {**classic, "args": ["--anti-knight"]}
        self.assertTrue(BENCHMARK.pg_supported(classic))
        self.assertFalse(BENCHMARK.pg_supported(variant))
        self.assertEqual(BENCHMARK.effective_pg_copies(classic, None), 1)
        self.assertEqual(BENCHMARK.effective_pg_copies(classic, 3), 3)
        self.assertEqual(BENCHMARK.effective_pg_timeout({"timeout_seconds": 90}, None), 90)
        self.assertEqual(
            BENCHMARK.effective_pg_timeout(
                {"timeout_seconds": 90, "pg_timeout_seconds": 120}, None
            ),
            120,
        )
        self.assertEqual(BENCHMARK.effective_pg_timeout(classic, 45), 45)

    def test_metadata_pins_the_reproducible_upstream_artifact(self) -> None:
        metadata = BENCHMARK.load_pg_metadata()
        self.assertEqual(
            metadata["commit"],
            "2f356d6cffbe45e1e7525c2df9ff383b861ada2d",
        )
        self.assertEqual(metadata["main_class"], "sudoku.serate")
        self.assertEqual(metadata["jar_size"], 87179)
        self.assertEqual(
            metadata["jar_sha256"],
            "f6e6e3707ba7e774d15125c886a60efffe15717015983757856b186e5a0df525",
        )

    def test_rating_validation_rejects_a_different_workload(self) -> None:
        case = {
            "id": "classic",
            "puzzle": "0" * 81,
            "expected_rating": "8.9/1.5/1.5",
        }
        with mock.patch.object(
            BENCHMARK, "run_checked", return_value="8.9/1.5/1.5\n"
        ):
            self.assertEqual(
                BENCHMARK.validate_pg_rating(
                    "java", Path("PGExplainer.jar"), "sudoku.serate", case, 60
                ),
                "8.9/1.5/1.5",
            )
        with mock.patch.object(
            BENCHMARK, "run_checked", return_value="9.0/1.5/1.5\n"
        ):
            with self.assertRaisesRegex(RuntimeError, "rates classic differently"):
                BENCHMARK.validate_pg_rating(
                    "java", Path("PGExplainer.jar"), "sudoku.serate", case, 60
                )


if __name__ == "__main__":
    unittest.main()
