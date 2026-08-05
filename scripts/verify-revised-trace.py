#!/usr/bin/env python3
"""Verify the frozen non-protected Revised-mode full-trace contract."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
from collections.abc import Callable, Mapping, Sequence
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BENCHMARK_SCRIPT = ROOT / "scripts" / "benchmark-java-rust.py"
BENCHMARK_SPEC = importlib.util.spec_from_file_location(
    "benchmark_java_rust", BENCHMARK_SCRIPT
)
if BENCHMARK_SPEC is None or BENCHMARK_SPEC.loader is None:
    raise RuntimeError(f"cannot load {BENCHMARK_SCRIPT}")
BENCHMARK = importlib.util.module_from_spec(BENCHMARK_SPEC)
BENCHMARK_SPEC.loader.exec_module(BENCHMARK)

REVISED_CASE_ID = "classic_dynamic_forcing_chain"
JAVA_REVISED_ARGUMENT = "--revisedRating=1"
RUST_REVISED_ARGUMENT = "--revised"
RUST_LABEL = "rust-debug"
CROSS_ENGINE_LABELS = ("java-original", "java-optimized", RUST_LABEL)
DEFAULT_RUST_BINARY = ROOT / "target" / "debug" / (
    "sukaku-forge.exe" if os.name == "nt" else "sukaku-forge"
)

EngineInvocation = tuple[str, list[str], str]
TimedRunner = Callable[[list[str], str, int], tuple[float, str]]


def parse_arguments(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cross-engine",
        action="store_true",
        help="also compare the pinned and optimized Java traces record by record",
    )
    parser.add_argument("--java", default=os.environ.get("SUKAKU_JAVA", "java"))
    parser.add_argument(
        "--optimized-jar", type=Path, default=BENCHMARK.DEFAULT_OPTIMIZED_JAR
    )
    parser.add_argument(
        "--original-jar", type=Path, default=BENCHMARK.DEFAULT_ORIGINAL_JAR
    )
    parser.add_argument("--rust-binary", type=Path, default=DEFAULT_RUST_BINARY)
    return parser.parse_args(argv)


def require_revised_case(document: Mapping[str, Any]) -> dict[str, Any]:
    """Load the one deliberately small, non-protected Revised trace fixture."""
    matches = [
        case for case in document["cases"] if case["id"] == REVISED_CASE_ID
    ]
    if len(matches) != 1:
        raise ValueError(
            f"benchmark document must contain exactly one {REVISED_CASE_ID}"
        )
    case = matches[0]
    if case.get("major_milestone_only"):
        raise ValueError("the Revised trace fixture must not be protected")
    if case.get("revised_expected_rating") is None:
        raise ValueError("the Revised trace fixture has no frozen rating")
    expected_trace = case.get("revised_expected_trace")
    if not isinstance(expected_trace, dict):
        raise ValueError("the Revised trace fixture has no frozen full-trace contract")
    required_fields = {
        "version",
        "step_count",
        "final_grid",
        "final_state_sha256",
        "sha256",
    }
    if set(expected_trace) != required_fields:
        raise ValueError(
            "the Revised trace contract fields changed: "
            f"{set(expected_trace)!r} != {required_fields!r}"
        )
    return case


def _with_argument(case: Mapping[str, Any], argument: str) -> dict[str, Any]:
    revised = dict(case)
    revised["args"] = [*case.get("args", []), argument]
    return revised


def engine_invocations(
    java: str,
    optimized_jar: Path,
    original_jar: Path,
    rust_binary: Path,
    main_class: str,
    case: Mapping[str, Any],
    cross_engine: bool,
) -> tuple[EngineInvocation, ...]:
    """Build one Revised invocation per selected engine."""
    java_case = _with_argument(case, JAVA_REVISED_ARGUMENT)
    rust_case = _with_argument(case, RUST_REVISED_ARGUMENT)
    puzzle_input = f"{case['puzzle']}\n"
    rust = (
        RUST_LABEL,
        BENCHMARK.rust_trace_command(rust_binary, rust_case),
        "",
    )
    if not cross_engine:
        return (rust,)
    return (
        (
            CROSS_ENGINE_LABELS[0],
            BENCHMARK.java_trace_command(
                java, original_jar, main_class, java_case
            ),
            puzzle_input,
        ),
        (
            CROSS_ENGINE_LABELS[1],
            BENCHMARK.java_trace_command(
                java, optimized_jar, main_class, java_case
            ),
            puzzle_input,
        ),
        rust,
    )


def require_artifacts(
    rust_binary: Path,
    cross_engine: bool,
    optimized_jar: Path,
    original_jar: Path,
) -> None:
    required = [rust_binary]
    if cross_engine:
        required.extend((original_jar, optimized_jar))
    for artifact in required:
        if not artifact.is_file():
            raise FileNotFoundError(f"artifact not found: {artifact}")
    if cross_engine:
        oracle = json.loads(BENCHMARK.DEFAULT_ORACLE.read_text(encoding="utf-8"))
        expected = oracle["oracle"]["sha256"]
        actual = BENCHMARK.sha256(original_jar)
        if actual != expected:
            raise RuntimeError(
                "java-original JAR does not match the frozen oracle: "
                f"{actual} != {expected}"
            )


def run_invocations_once(
    invocations: Sequence[EngineInvocation],
    timeout: int,
    runner: TimedRunner = BENCHMARK.time_command_with_output,
) -> dict[str, dict[str, Any]]:
    """Run every selected engine exactly once and keep its canonical records."""
    labels = tuple(label for label, _command, _payload in invocations)
    if len(labels) != len(set(labels)) or labels not in (
        (RUST_LABEL,),
        CROSS_ENGINE_LABELS,
    ):
        raise ValueError(f"unexpected Revised trace engine order: {labels!r}")
    results: dict[str, dict[str, Any]] = {}
    for label, command, payload in invocations:
        elapsed, stdout = runner(command, payload, timeout)
        results[label] = {
            "elapsed": elapsed,
            "trace": BENCHMARK.parse_compact_trace(
                stdout, f"{label} {REVISED_CASE_ID} Revised"
            ),
        }
    return results


def require_frozen_trace(
    case: Mapping[str, Any], label: str, trace: Mapping[str, Any]
) -> None:
    expected_rating = case["revised_expected_rating"]
    if trace["rating"] != expected_rating:
        raise RuntimeError(
            f"{label} Revised rating changed: "
            f"{trace['rating']} != {expected_rating}"
        )
    BENCHMARK.require_trace_contract(
        f"{label} {REVISED_CASE_ID} Revised",
        trace,
        case["revised_expected_trace"],
    )


def _record_summary(record: str | None) -> str:
    if record is None:
        return "<missing>"
    fields = record.split("\t", 4)
    if fields[0] == "STEP" and len(fields) >= 3:
        return f"STEP rating={fields[1]} description={fields[2]!r}"
    return record


def require_exact_consensus(
    results: Mapping[str, Mapping[str, Any]],
) -> None:
    """Compare all canonical records, not merely the derived digests."""
    if tuple(results) != CROSS_ENGINE_LABELS:
        raise RuntimeError(
            "cross-engine Revised result set changed: "
            f"{tuple(results)!r} != {CROSS_ENGINE_LABELS!r}"
        )
    reference_label = CROSS_ENGINE_LABELS[0]
    reference = results[reference_label]["trace"]
    for label in CROSS_ENGINE_LABELS[1:]:
        actual = results[label]["trace"]
        if actual["records"] != reference["records"]:
            expected_records = reference["records"]
            actual_records = actual["records"]
            common = min(len(expected_records), len(actual_records))
            mismatch = next(
                (
                    index
                    for index in range(common)
                    if expected_records[index] != actual_records[index]
                ),
                common,
            )
            expected_record = (
                expected_records[mismatch]
                if mismatch < len(expected_records)
                else None
            )
            actual_record = (
                actual_records[mismatch] if mismatch < len(actual_records) else None
            )
            raise RuntimeError(
                f"Revised canonical record mismatch at record {mismatch + 1}: "
                f"{reference_label}={_record_summary(expected_record)}, "
                f"{label}={_record_summary(actual_record)}"
            )
        if actual["contract"] != reference["contract"]:
            raise RuntimeError(
                f"Revised v1 contract mismatch: {reference_label}="
                f"{reference['contract']!r}, {label}={actual['contract']!r}"
            )


def print_results(results: Mapping[str, Mapping[str, Any]]) -> None:
    for label, result in results.items():
        trace = result["trace"]
        contract = trace["contract"]
        print(
            f"{label}: elapsed={result['elapsed']:.3f}s; "
            f"steps={contract['step_count']}; rating={trace['rating']}; "
            f"digest={contract['sha256']}"
        )


def main(argv: Sequence[str] | None = None) -> int:
    arguments = parse_arguments(argv)
    document = json.loads(BENCHMARK.DEFAULT_CASES.read_text(encoding="utf-8"))
    case = require_revised_case(document)
    rust_binary = arguments.rust_binary.resolve()
    optimized_jar = arguments.optimized_jar.resolve()
    original_jar = arguments.original_jar.resolve()
    require_artifacts(
        rust_binary, arguments.cross_engine, optimized_jar, original_jar
    )
    invocations = engine_invocations(
        arguments.java,
        optimized_jar,
        original_jar,
        rust_binary,
        document["main_class"],
        case,
        arguments.cross_engine,
    )
    results = run_invocations_once(
        invocations, case.get("timeout_seconds", 30)
    )
    for label, result in results.items():
        require_frozen_trace(case, label, result["trace"])
    if arguments.cross_engine:
        require_exact_consensus(results)
    print(f"Revised full-trace case: {REVISED_CASE_ID}")
    print_results(results)
    print("PASS frozen Revised-mode full-trace contract")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (FileNotFoundError, OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
