#!/usr/bin/env python3
"""Static and synthetic tests for the Revised-mode full-trace verifier."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("verify-revised-trace.py")
SPEC = importlib.util.spec_from_file_location("verify_revised_trace", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


def compact_output(description: str = "Example") -> str:
    grid = "1" * 81
    candidates = "." * 729
    return (
        f"STEP\t2.0\t{description}\t{grid}\t{candidates}\n"
        "RESULT\t2.0\t1.0\t1.0\tExample\tHidden Single\tHidden Single\n"
    )


def result_set(trace: dict) -> dict[str, dict]:
    return {
        label: {"elapsed": float(index), "trace": trace}
        for index, label in enumerate(VERIFIER.CROSS_ENGINE_LABELS, 1)
    }


class RevisedTraceFixtureTests(unittest.TestCase):
    def setUp(self) -> None:
        self.document = json.loads(
            VERIFIER.BENCHMARK.DEFAULT_CASES.read_text(encoding="utf-8")
        )
        self.case = VERIFIER.require_revised_case(self.document)

    def test_fixture_is_non_protected_and_frozen(self) -> None:
        self.assertEqual(self.case["id"], "classic_dynamic_forcing_chain")
        self.assertFalse(self.case.get("major_milestone_only", False))
        self.assertEqual(self.case["revised_expected_rating"], "8.9/1.5/1.5")
        self.assertEqual(
            self.case["revised_expected_trace"],
            {
                "version": 1,
                "step_count": 72,
                "final_grid": (
                    "183594762526378149479261583231689457865743291794125836"
                    "648917325312856974957432618"
                ),
                "final_state_sha256": (
                    "bfde6161d2ed4952149e73e22a4cf8eb786cbdb94aadda7302eb1bcdfcf97be7"
                ),
                "sha256": (
                    "1d22000f59ad2837570bbc17f1298bdfcd9c79483fc2a97eb099651c8d8570a6"
                ),
            },
        )

    def test_cross_engine_commands_select_revised_mode_explicitly(self) -> None:
        invocations = VERIFIER.engine_invocations(
            "java",
            Path("optimized.jar"),
            Path("original.jar"),
            Path("sukaku-forge"),
            self.document["main_class"],
            self.case,
            True,
        )
        self.assertEqual(
            tuple(label for label, _command, _payload in invocations),
            VERIFIER.CROSS_ENGINE_LABELS,
        )
        for label, command, payload in invocations:
            if label.startswith("java-"):
                self.assertEqual(command.count("--revisedRating=1"), 1)
                self.assertNotIn("--revised", command)
                self.assertEqual(payload, f"{self.case['puzzle']}\n")
            else:
                self.assertEqual(command.count("--revised"), 1)
                self.assertNotIn("--revisedRating=1", command)
                self.assertEqual(payload, "")

    def test_default_invocation_is_one_rust_solve(self) -> None:
        invocations = VERIFIER.engine_invocations(
            "java",
            Path("optimized.jar"),
            Path("original.jar"),
            Path("sukaku-forge"),
            self.document["main_class"],
            self.case,
            False,
        )
        calls: list[list[str]] = []

        def runner(command: list[str], _payload: str, _timeout: int):
            calls.append(command)
            return 0.1, compact_output()

        results = VERIFIER.run_invocations_once(invocations, 1, runner)
        self.assertEqual(tuple(results), (VERIFIER.RUST_LABEL,))
        self.assertEqual(len(calls), 1)

    def test_protected_fixture_is_rejected(self) -> None:
        changed = json.loads(json.dumps(self.document))
        case = next(
            case
            for case in changed["cases"]
            if case["id"] == VERIFIER.REVISED_CASE_ID
        )
        case["major_milestone_only"] = True
        with self.assertRaisesRegex(ValueError, "must not be protected"):
            VERIFIER.require_revised_case(changed)


class RevisedTraceComparisonTests(unittest.TestCase):
    def test_frozen_contract_and_exact_consensus_accept_identical_records(self) -> None:
        trace = VERIFIER.BENCHMARK.parse_compact_trace(compact_output(), "test")
        case = {
            "revised_expected_rating": trace["rating"],
            "revised_expected_trace": trace["contract"],
        }
        VERIFIER.require_frozen_trace(case, "test", trace)
        VERIFIER.require_exact_consensus(result_set(trace))

    def test_first_changed_record_is_reported(self) -> None:
        trace = VERIFIER.BENCHMARK.parse_compact_trace(compact_output(), "test")
        results = result_set(trace)
        changed = dict(trace)
        changed_records = list(trace["records"])
        changed_records[0] = changed_records[0].replace("Example", "Changed")
        changed["records"] = tuple(changed_records)
        results[VERIFIER.RUST_LABEL] = {"elapsed": 3.0, "trace": changed}
        with self.assertRaisesRegex(RuntimeError, "record mismatch at record 1"):
            VERIFIER.require_exact_consensus(results)

    def test_frozen_digest_change_is_rejected(self) -> None:
        trace = VERIFIER.BENCHMARK.parse_compact_trace(compact_output(), "test")
        case = {
            "revised_expected_rating": trace["rating"],
            "revised_expected_trace": dict(trace["contract"]),
        }
        case["revised_expected_trace"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(RuntimeError, "trace contract changed"):
            VERIFIER.require_frozen_trace(case, "test", trace)


if __name__ == "__main__":
    unittest.main()
