#!/usr/bin/env python3
"""Verify the protected 11.8 full trace once across all three engines."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import sys
from collections.abc import Callable, Mapping, Sequence
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
BENCHMARK_SCRIPT = SCRIPT_DIR / "benchmark-java-rust.py"
BENCHMARK_SPEC = importlib.util.spec_from_file_location(
    "benchmark_java_rust", BENCHMARK_SCRIPT
)
if BENCHMARK_SPEC is None or BENCHMARK_SPEC.loader is None:
    raise RuntimeError(f"cannot load {BENCHMARK_SCRIPT}")
BENCHMARK = importlib.util.module_from_spec(BENCHMARK_SPEC)
BENCHMARK_SPEC.loader.exec_module(BENCHMARK)

PROTECTED_CASE_ID = "user_extreme_major_milestone_probe"
ENGINE_LABELS = ("java-original", "java-optimized", "rust-release")
SHA256_PATTERN = re.compile(r"[0-9a-f]{64}")

EngineInvocation = tuple[str, list[str], str]
TimedRunner = Callable[[list[str], str, int], tuple[float, str]]


def parse_arguments(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--case",
        action="append",
        dest="case_ids",
        required=True,
        help=f"must be exactly {PROTECTED_CASE_ID}",
    )
    parser.add_argument(
        "--allow-major-milestone",
        action="store_true",
        help="acknowledge the protected, long-running full-trace verification",
    )
    parser.add_argument("--java", default=os.environ.get("SUKAKU_JAVA", "java"))
    parser.add_argument(
        "--optimized-jar", type=Path, default=BENCHMARK.DEFAULT_OPTIMIZED_JAR
    )
    parser.add_argument(
        "--original-jar", type=Path, default=BENCHMARK.DEFAULT_ORIGINAL_JAR
    )
    parser.add_argument(
        "--rust-binary", type=Path, default=BENCHMARK.DEFAULT_RUST_BINARY
    )
    return parser.parse_args(argv)


def require_authorized_case(
    document: Mapping[str, Any],
    selected_case_ids: Sequence[str],
    allow_major_milestone: bool,
) -> dict[str, Any]:
    """Return only the protected shared case after both explicit gates pass."""
    if list(selected_case_ids) != [PROTECTED_CASE_ID] or not allow_major_milestone:
        raise ValueError(
            f"{PROTECTED_CASE_ID} requires exact --case plus "
            "--allow-major-milestone"
        )
    matches = [
        case for case in document["cases"] if case["id"] == PROTECTED_CASE_ID
    ]
    if len(matches) != 1:
        raise ValueError(
            f"shared benchmark document must contain exactly one {PROTECTED_CASE_ID}"
        )
    case = matches[0]
    copies = BENCHMARK.effective_copies(case, None)
    BENCHMARK.require_major_milestone_policy(case, True, True, 1, copies)
    if not case.get("major_milestone_only"):
        raise ValueError(f"{PROTECTED_CASE_ID} is not marked major_milestone_only")
    if case.get("expected_rating") is None:
        raise ValueError(f"{PROTECTED_CASE_ID} has no frozen expected rating")
    if case.get("original_expected_rating") != case["expected_rating"]:
        raise ValueError(
            f"{PROTECTED_CASE_ID} original and cross-engine ratings are not frozen "
            "to the same value"
        )
    return case


def require_sha256_match(actual: str, expected: str) -> None:
    """Require a well-formed exact match for the pinned original JAR digest."""
    if SHA256_PATTERN.fullmatch(expected) is None:
        raise RuntimeError(f"malformed frozen original JAR SHA-256: {expected!r}")
    if SHA256_PATTERN.fullmatch(actual) is None or actual != expected:
        raise RuntimeError(
            "java-original JAR does not match the frozen oracle: "
            f"{actual} != {expected}"
        )


def require_artifacts(
    optimized_jar: Path, original_jar: Path, rust_binary: Path
) -> None:
    for artifact in (original_jar, optimized_jar, rust_binary):
        if not artifact.is_file():
            raise FileNotFoundError(f"artifact not found: {artifact}")
    oracle_document = json.loads(BENCHMARK.DEFAULT_ORACLE.read_text(encoding="utf-8"))
    expected_original_hash = oracle_document["oracle"]["sha256"]
    require_sha256_match(BENCHMARK.sha256(original_jar), expected_original_hash)


def engine_invocations(
    java: str,
    optimized_jar: Path,
    original_jar: Path,
    rust_binary: Path,
    main_class: str,
    case: dict[str, Any],
) -> tuple[EngineInvocation, ...]:
    """Build the fixed original-Java, optimized-Java, Rust execution order."""
    puzzle_input = case["puzzle"] + "\n"
    return (
        (
            ENGINE_LABELS[0],
            BENCHMARK.java_trace_command(java, original_jar, main_class, case),
            puzzle_input,
        ),
        (
            ENGINE_LABELS[1],
            BENCHMARK.java_trace_command(java, optimized_jar, main_class, case),
            puzzle_input,
        ),
        (
            ENGINE_LABELS[2],
            BENCHMARK.rust_trace_command(rust_binary, case),
            "",
        ),
    )


def run_traces_once_sequentially(
    invocations: Sequence[EngineInvocation],
    timeout: int,
    runner: TimedRunner = BENCHMARK.time_command_with_output,
) -> dict[str, dict[str, Any]]:
    """Run each solver once, in order, retaining no trace file on disk."""
    labels = tuple(label for label, _command, _payload in invocations)
    if labels != ENGINE_LABELS:
        raise ValueError(
            f"protected trace engine order changed: {labels!r} != {ENGINE_LABELS!r}"
        )
    results: dict[str, dict[str, Any]] = {}
    for label, command, payload in invocations:
        elapsed, stdout = runner(command, payload, timeout)
        trace = BENCHMARK.parse_compact_trace(
            stdout, f"{label} {PROTECTED_CASE_ID}"
        )
        results[label] = {"elapsed": elapsed, "trace": trace}
        del stdout
    return results


def _record_summary(record: str | None) -> str:
    if record is None:
        return "<missing>"
    fields = record.split("\t", 4)
    if fields[0] == "STEP" and len(fields) >= 3:
        return f"STEP rating={fields[1]} description={fields[2]!r}"
    return record


def require_exact_consensus(
    results: Mapping[str, Mapping[str, Any]], expected_rating: str
) -> dict[str, Any]:
    """Compare every canonical record and all derived v1 contract fields."""
    labels = tuple(results)
    if labels != ENGINE_LABELS:
        raise RuntimeError(
            f"incomplete protected trace result set: {labels!r} != {ENGINE_LABELS!r}"
        )
    traces = {label: results[label]["trace"] for label in ENGINE_LABELS}
    for label, trace in traces.items():
        if trace["rating"] != expected_rating:
            raise RuntimeError(
                f"{label} protected trace rating changed: "
                f"{trace['rating']} != {expected_rating}"
            )

    reference_label = ENGINE_LABELS[0]
    reference = traces[reference_label]
    reference_records = reference["records"]
    for label in ENGINE_LABELS[1:]:
        actual = traces[label]
        actual_records = actual["records"]
        if actual_records != reference_records:
            common = min(len(reference_records), len(actual_records))
            mismatch = next(
                (
                    index
                    for index in range(common)
                    if reference_records[index] != actual_records[index]
                ),
                common,
            )
            expected_record = (
                reference_records[mismatch]
                if mismatch < len(reference_records)
                else None
            )
            actual_record = (
                actual_records[mismatch] if mismatch < len(actual_records) else None
            )
            raise RuntimeError(
                f"canonical trace record mismatch at record {mismatch + 1}: "
                f"{reference_label}={_record_summary(expected_record)}, "
                f"{label}={_record_summary(actual_record)}"
            )
        if actual["result"] != reference["result"]:
            raise RuntimeError(
                f"RESULT mismatch: {reference_label}={reference['result']!r}, "
                f"{label}={actual['result']!r}"
            )
        if actual["rating"] != reference["rating"]:
            raise RuntimeError(
                f"rating mismatch: {reference_label}={reference['rating']}, "
                f"{label}={actual['rating']}"
            )
        if actual["contract"] != reference["contract"]:
            raise RuntimeError(
                f"v1 trace contract mismatch: {reference_label}="
                f"{reference['contract']!r}, {label}={actual['contract']!r}"
            )
    return reference


def require_frozen_trace_if_present(
    case: Mapping[str, Any], trace: dict[str, Any]
) -> None:
    """Allow the first capture, then enforce a case-level contract once frozen."""
    expected_trace = case.get("expected_trace")
    if expected_trace is not None:
        BENCHMARK.require_trace_contract(
            f"protected {case['id']}", trace, expected_trace
        )


def print_results(results: Mapping[str, Mapping[str, Any]]) -> None:
    for label in ENGINE_LABELS:
        elapsed = results[label]["elapsed"]
        trace = results[label]["trace"]
        contract = trace["contract"]
        print(
            f"{label}: elapsed={elapsed:.3f}s; "
            f"step_count={contract['step_count']}; result={trace['result']!r}; "
            f"final_grid={contract['final_grid']}; "
            f"digest={contract['sha256']}; "
            f"final_state_digest={contract['final_state_sha256']}"
        )


def main(argv: Sequence[str] | None = None) -> int:
    arguments = parse_arguments(argv)
    document = json.loads(BENCHMARK.DEFAULT_CASES.read_text(encoding="utf-8"))
    case = require_authorized_case(
        document, arguments.case_ids, arguments.allow_major_milestone
    )

    optimized_jar = arguments.optimized_jar.resolve()
    original_jar = arguments.original_jar.resolve()
    rust_binary = arguments.rust_binary.resolve()
    require_artifacts(optimized_jar, original_jar, rust_binary)

    invocations = engine_invocations(
        arguments.java,
        optimized_jar,
        original_jar,
        rust_binary,
        document["main_class"],
        case,
    )
    print(f"protected full-trace case: {case['id']}")
    print("policy: one sequential solve per engine; stdout is retained in memory only")
    results = run_traces_once_sequentially(
        invocations, case.get("timeout_seconds", 60)
    )
    reference = require_exact_consensus(results, case["expected_rating"])
    require_frozen_trace_if_present(case, reference)
    print_results(results)
    print(
        "exact consensus: all canonical v1 records, rating, final state and "
        f"digest agree ({reference['contract']['sha256']})"
    )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (FileNotFoundError, OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
