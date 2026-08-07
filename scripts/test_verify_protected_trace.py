#!/usr/bin/env python3
"""Static and synthetic tests for the protected full-trace verifier."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("verify-protected-trace.py")
SPEC = importlib.util.spec_from_file_location("verify_protected_trace", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


def compact_output(description: str = "Example: r1c1.1 off") -> str:
    grid = "1" * 81
    candidates = "." * 729
    return (
        f"STEP\t11.8\t{description}\t{grid}\t{candidates}\n"
        "RESULT\t11.8\t1.2\t1.2\tDynamic Forcing Chain\t"
        "Hidden Single\tNaked Single\n"
    )


def result_set(trace: dict) -> dict[str, dict]:
    return {
        label: {"elapsed": float(index), "trace": trace}
        for index, label in enumerate(VERIFIER.ENGINE_LABELS, start=1)
    }


class ProtectedTraceGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.document = json.loads(
            VERIFIER.BENCHMARK.DEFAULT_CASES.read_text(encoding="utf-8")
        )

    def test_exact_case_and_explicit_acknowledgement_are_both_required(self) -> None:
        with self.assertRaisesRegex(ValueError, "exact --case"):
            VERIFIER.require_authorized_case(
                self.document, [VERIFIER.PROTECTED_CASE_ID], False
            )
        with self.assertRaisesRegex(ValueError, "exact --case"):
            VERIFIER.require_authorized_case(
                self.document, ["forum_hard_10_5_probe"], True
            )
        with self.assertRaisesRegex(ValueError, "exact --case"):
            VERIFIER.require_authorized_case(
                self.document,
                [VERIFIER.PROTECTED_CASE_ID, VERIFIER.PROTECTED_CASE_ID],
                True,
            )
        with self.assertRaisesRegex(ValueError, "exact --case"):
            VERIFIER.require_authorized_case(
                self.document,
                ["forum_hard_10_5_probe", VERIFIER.PROTECTED_CASE_ID],
                True,
            )

        case = VERIFIER.require_authorized_case(
            self.document, [VERIFIER.PROTECTED_CASE_ID], True
        )
        self.assertTrue(case["major_milestone_only"])
        self.assertEqual(case["copies"], 1)
        self.assertEqual(case["expected_rating"], "11.8/1.2/1.2")

    def test_pinned_comparator_digest_requires_an_exact_sha256(self) -> None:
        digest = "a" * 64
        VERIFIER.require_sha256_match(digest, digest)
        with self.assertRaisesRegex(RuntimeError, "malformed pinned"):
            VERIFIER.require_sha256_match(digest, "not-a-digest")
        with self.assertRaisesRegex(RuntimeError, "does not match"):
            VERIFIER.require_sha256_match("b" * 64, digest)

    def test_engine_plan_has_one_fixed_sequential_invocation_per_engine(self) -> None:
        case = VERIFIER.require_authorized_case(
            self.document, [VERIFIER.PROTECTED_CASE_ID], True
        )
        invocations = VERIFIER.engine_invocations(
            "java",
            Path("optimized.jar"),
            Path("original.jar"),
            Path("sukaku-forge"),
            self.document["main_class"],
            case,
        )
        self.assertEqual(
            tuple(label for label, _command, _payload in invocations),
            VERIFIER.ENGINE_LABELS,
        )
        for _label, command, payload in invocations[:2]:
            self.assertIn(f"--after={VERIFIER.BENCHMARK.TRACE_STEP_FORMAT}", command)
            self.assertIn(
                f"--format={VERIFIER.BENCHMARK.TRACE_RESULT_FORMAT}", command
            )
            self.assertEqual(payload, case["puzzle"] + "\n")
        self.assertEqual(invocations[2][2], "")
        self.assertEqual(invocations[2][1][-1], case["puzzle"])


class ProtectedTraceContractTests(unittest.TestCase):
    def test_runner_is_called_once_per_engine_in_order_and_keeps_no_stdout(self) -> None:
        calls: list[tuple[list[str], str, int]] = []

        def fake_runner(
            command: list[str], payload: str, timeout: int
        ) -> tuple[float, str]:
            calls.append((command, payload, timeout))
            return float(len(calls)), compact_output()

        invocations = tuple(
            (label, [label, "trace"], f"payload-{label}")
            for label in VERIFIER.ENGINE_LABELS
        )
        results = VERIFIER.run_traces_once_sequentially(
            invocations, 123, runner=fake_runner
        )

        self.assertEqual(
            [command[0] for command, _payload, _timeout in calls],
            list(VERIFIER.ENGINE_LABELS),
        )
        self.assertEqual(len(calls), 3)
        self.assertTrue(all(timeout == 123 for _command, _payload, timeout in calls))
        self.assertNotIn("stdout", results[VERIFIER.ENGINE_LABELS[0]])
        VERIFIER.require_exact_consensus(results, "11.8/1.2/1.2")

    def test_every_canonical_record_is_compared_directly(self) -> None:
        trace = VERIFIER.BENCHMARK.parse_compact_trace(compact_output(), "test")
        self.assertIsInstance(trace["records"], tuple)
        self.assertEqual(trace["result"], trace["records"][-1])
        results = result_set(trace)

        changed = dict(trace)
        changed_records = list(trace["records"])
        changed_records[0] = changed_records[0].replace("Example", "Changed", 1)
        changed["records"] = tuple(changed_records)
        results["rust-release"] = {"elapsed": 3.0, "trace": changed}
        with self.assertRaisesRegex(RuntimeError, "record mismatch at record 1"):
            VERIFIER.require_exact_consensus(results, "11.8/1.2/1.2")

    def test_rating_final_state_and_digest_contract_are_compared(self) -> None:
        trace = VERIFIER.BENCHMARK.parse_compact_trace(compact_output(), "test")
        results = result_set(trace)
        changed = dict(trace)
        changed["contract"] = dict(trace["contract"])
        changed["contract"]["final_state_sha256"] = "0" * 64
        results["rust-release"] = {"elapsed": 3.0, "trace": changed}
        with self.assertRaisesRegex(RuntimeError, "v1 trace contract mismatch"):
            VERIFIER.require_exact_consensus(results, "11.8/1.2/1.2")

        with self.assertRaisesRegex(RuntimeError, "rating changed"):
            VERIFIER.require_exact_consensus(result_set(trace), "11.9/1.2/1.2")

    def test_first_capture_needs_no_frozen_trace_and_future_capture_does(self) -> None:
        trace = VERIFIER.BENCHMARK.parse_compact_trace(compact_output(), "test")
        case = {"id": VERIFIER.PROTECTED_CASE_ID}
        VERIFIER.require_frozen_trace_if_present(case, trace)

        case["expected_trace"] = dict(trace["contract"])
        VERIFIER.require_frozen_trace_if_present(case, trace)
        case["expected_trace"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(RuntimeError, "trace contract changed"):
            VERIFIER.require_frozen_trace_if_present(case, trace)

    def test_report_contains_elapsed_steps_result_and_both_digests(self) -> None:
        trace = VERIFIER.BENCHMARK.parse_compact_trace(compact_output(), "test")
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            VERIFIER.print_results(result_set(trace))
        report = output.getvalue()
        self.assertEqual(report.count("elapsed="), 3)
        self.assertEqual(report.count("step_count="), 3)
        self.assertEqual(report.count("result="), 3)
        self.assertEqual(report.count("final_grid="), 3)
        self.assertEqual(report.count("digest="), 6)


if __name__ == "__main__":
    unittest.main()
