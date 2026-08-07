#!/usr/bin/env python3
"""Compare Forge with the Java oracles and the classic-only PGExplainer rater."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
JAVA_ROOT = ROOT.parent / "sukaku-explainer"
DEFAULT_CASES = JAVA_ROOT / "benchmarks" / "cases.json"
DEFAULT_ORACLE = JAVA_ROOT / "oracle" / "cases.json"
DEFAULT_RUST_BINARY = ROOT / "target" / "release" / (
    "sukaku-forge.exe" if os.name == "nt" else "sukaku-forge"
)
DEFAULT_OPTIMIZED_JAR = JAVA_ROOT / "build" / "SukakuExplainer.jar"
DEFAULT_ORIGINAL_JAR = (
    ROOT / "target" / "sudokumonster" / "SukakuExplainer-v1.18.1.jar"
)
DEFAULT_PG_JAR = ROOT / "target" / "pgexplainer" / "PGExplainer.jar"
SUDOKUMONSTER_V118_METADATA = ROOT / "scripts" / "sudokumonster-v118.json"
PG_METADATA = ROOT / "scripts" / "pgexplainer.json"
PG_LABEL = "pg-upstream-parallel"
DEFAULT_CASE_IDS = (
    "classic_dynamic_forcing_chain",
    "anti_knight_forcing_chain",
)
TRACE_CONTRACT_VERSION = 1
TRACE_STEP_FORMAT = "STEP%t%r%t%s%t%i%t%m"
TRACE_RESULT_FORMAT = "RESULT%t%r%t%p%t%d%t%R%t%P%t%D"
RATING_PATTERN = re.compile(r"\d+(?:\.\d+)?/\d+(?:\.\d+)?/\d+(?:\.\d+)?")
UNFROZEN_RATING_ENGINES = (
    "java-original",
    "java-optimized",
    "rust-release",
)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, default=DEFAULT_CASES)
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--case", action="append", dest="case_ids")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--copies", type=int)
    parser.add_argument("--java", default=os.environ.get("SUKAKU_JAVA", "java"))
    parser.add_argument("--optimized-jar", type=Path, default=DEFAULT_OPTIMIZED_JAR)
    parser.add_argument("--original-jar", type=Path, default=DEFAULT_ORIGINAL_JAR)
    parser.add_argument("--rust-binary", type=Path, default=DEFAULT_RUST_BINARY)
    parser.add_argument("--pg-jar", type=Path, default=DEFAULT_PG_JAR)
    parser.add_argument(
        "--without-pg",
        action="store_true",
        help="omit the optional classic-only PGExplainer comparison",
    )
    parser.add_argument(
        "--pg-timeout",
        type=int,
        help="override the PGExplainer timeout in seconds",
    )
    parser.add_argument(
        "--allow-major-milestone",
        action="store_true",
        help="allow an explicitly selected protected major-milestone case",
    )
    return parser.parse_args()


def load_cases(path: Path, selected_ids: list[str]) -> tuple[dict, list[dict]]:
    document = json.loads(path.read_text(encoding="utf-8"))
    by_id = {case["id"]: case for case in document["cases"]}
    missing = [case_id for case_id in selected_ids if case_id not in by_id]
    if missing:
        raise ValueError(f"unknown benchmark case(s): {', '.join(missing)}")
    return document, [by_id[case_id] for case_id in selected_ids]


def java_command(java: str, jar: Path, main_class: str, case: dict, output: str) -> list[str]:
    return [
        java,
        "-cp",
        str(jar),
        main_class,
        "--threads=1",
        *case.get("args", []),
        f"--format={output}",
        "--input=-",
    ]


def pg_command(java: str, jar: Path, main_class: str, output: str) -> list[str]:
    """Build PG's intentionally separate CLI without unsupported SE flags."""
    return [
        java,
        "-Xrs",
        "-Xmx500m",
        "-cp",
        str(jar),
        main_class,
        f"--format={output}",
        "--input=-",
    ]


def pg_supported(case: dict) -> bool:
    """PGExplainer is a classic 9x9 rater and silently ignores variant flags."""
    return not case.get("args")


def rust_command(binary: Path, case: dict, quiet: bool = False) -> list[str]:
    return [
        str(binary),
        "batch-rate",
        *(["--quiet"] if quiet else []),
        *case.get("args", []),
    ]


def java_trace_command(java: str, jar: Path, main_class: str, case: dict) -> list[str]:
    return [
        java,
        "-cp",
        str(jar),
        main_class,
        "--threads=1",
        *case.get("args", []),
        f"--after={TRACE_STEP_FORMAT}",
        f"--format={TRACE_RESULT_FORMAT}",
        "--input=-",
    ]


def rust_trace_command(binary: Path, case: dict) -> list[str]:
    return [str(binary), "trace", *case.get("args", []), case["puzzle"]]


def run_checked(command: list[str], payload: str, timeout: int) -> str:
    try:
        completed = subprocess.run(
            command,
            input=payload,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(
            f"command timed out after {timeout}s ({' '.join(command)})"
        ) from error
    if completed.returncode != 0:
        raise RuntimeError(
            f"command failed ({' '.join(command)}): {completed.stderr.strip()}"
        )
    return completed.stdout.strip()


def last_output_line(output: str) -> str:
    """Return the payload line after any JVM diagnostic preamble."""
    lines = [line.strip() for line in output.splitlines() if line.strip()]
    return lines[-1] if lines else ""


def require_rating_format(rating: str, label: str) -> str:
    if RATING_PATTERN.fullmatch(rating) is None:
        raise RuntimeError(f"malformed {label} rating: {rating!r}")
    return rating


def parse_java_rating(output: str, label: str) -> str:
    """Read a %r/%p/%d rating after any JVM diagnostic preamble."""
    return require_rating_format(last_output_line(output), label)


def parse_rust_rating(output: str, label: str) -> str:
    """Read the single RESULT rating emitted for one Rust input puzzle."""
    results = [line for line in output.splitlines() if line.startswith("RESULT\t")]
    if len(results) != 1:
        raise RuntimeError(
            f"malformed {label} rating: expected one RESULT, got {len(results)}"
        )
    fields = results[0].split("\t")
    if len(fields) != 7:
        raise RuntimeError(f"malformed {label} RESULT: {fields!r}")
    return require_rating_format("/".join(fields[1:4]), label)


def require_rating_consensus(case_id: str, ratings: dict[str, str]) -> str:
    missing = [label for label in UNFROZEN_RATING_ENGINES if label not in ratings]
    unexpected = [label for label in ratings if label not in UNFROZEN_RATING_ENGINES]
    if missing or unexpected:
        raise RuntimeError(
            f"incomplete unfrozen rating set for {case_id}: "
            f"missing={missing!r}, unexpected={unexpected!r}"
        )
    distinct = set(ratings.values())
    if len(distinct) != 1:
        details = ", ".join(
            f"{label}={ratings[label]}" for label in UNFROZEN_RATING_ENGINES
        )
        raise RuntimeError(f"unfrozen rating disagreement for {case_id}: {details}")
    return ratings[UNFROZEN_RATING_ENGINES[0]]


def effective_copies(case: dict, requested_copies: int | None) -> int:
    return case["copies"] if requested_copies is None else requested_copies


def effective_pg_copies(case: dict, requested_copies: int | None) -> int:
    """Use one PG solve by default; an explicit --copies still overrides all."""
    return case.get("pg_copies", 1) if requested_copies is None else requested_copies


def effective_pg_timeout(case: dict, requested_timeout: int | None) -> int:
    if requested_timeout is not None:
        return requested_timeout
    return case.get("pg_timeout_seconds", case.get("timeout_seconds", 60))


def uses_single_solve_rating_path(case: dict) -> bool:
    """Protected cases always use their timed run as their only solve."""
    return bool(case.get("major_milestone_only"))


def require_frozen_major_milestone_rating(case: dict, rating: str) -> None:
    expected = case.get("expected_rating")
    if expected is not None and rating != expected:
        raise RuntimeError(
            f"major-milestone rating changed for {case['id']}: "
            f"{rating} != {expected}"
        )


def require_major_milestone_policy(
    case: dict,
    explicitly_selected: bool,
    allow_major_milestone: bool,
    runs: int,
    copies: int,
) -> None:
    if not case.get("major_milestone_only"):
        return
    if not explicitly_selected or not allow_major_milestone:
        raise ValueError(
            f"{case['id']} requires exact --case plus --allow-major-milestone"
        )
    if runs != 1:
        raise ValueError(f"{case['id']} requires --runs 1")
    if copies != 1:
        raise ValueError(f"{case['id']} requires effective --copies 1")


def parse_compact_trace(output: str, label: str) -> dict:
    """Reduce a Java or Rust trace to the stable version-1 digest contract."""
    steps: list[list[str]] = []
    records: list[str] = []
    result: list[str] | None = None
    for line in output.splitlines():
        if not line:
            continue
        if line.startswith("STEP\t"):
            if result is not None:
                raise RuntimeError(f"{label} emitted a STEP after RESULT")
            fields = line.split("\t", 4)
            if len(fields) != 5:
                raise RuntimeError(f"malformed {label} STEP: {line!r}")
            if len(fields[3]) != 81 or len(fields[4]) != 729:
                raise RuntimeError(
                    f"malformed {label} STEP state: expected 81 grid and "
                    f"729 candidate characters, got {len(fields[3])} and "
                    f"{len(fields[4])}"
                )
            steps.append(fields)
            records.append("\t".join(fields))
        elif line.startswith("RESULT\t"):
            if result is not None:
                raise RuntimeError(f"{label} emitted more than one RESULT")
            fields = line.split("\t")
            if len(fields) != 7:
                raise RuntimeError(f"malformed {label} RESULT: {line!r}")
            result = fields
            records.append("\t".join(fields))
        else:
            raise RuntimeError(f"unexpected {label} output: {line!r}")
    if not steps:
        raise RuntimeError(f"{label} emitted no STEP records")
    if result is None:
        raise RuntimeError(f"{label} emitted no RESULT")

    final_grid = steps[-1][3]
    final_candidates = steps[-1][4]
    canonical = ("\n".join(records) + "\n").encode("utf-8")
    final_state = f"{final_grid}\n{final_candidates}\n".encode("utf-8")
    return {
        "contract": {
            "version": TRACE_CONTRACT_VERSION,
            "step_count": len(steps),
            "final_grid": final_grid,
            "final_state_sha256": hashlib.sha256(final_state).hexdigest(),
            "sha256": hashlib.sha256(canonical).hexdigest(),
        },
        "rating": "/".join(result[1:4]),
        "records": tuple(records),
        "result": records[-1],
    }


def require_trace_contract(label: str, actual: dict, expected: dict) -> None:
    version = expected.get("version")
    if version != TRACE_CONTRACT_VERSION:
        raise RuntimeError(
            f"unsupported trace contract version for {label}: "
            f"{version!r} != {TRACE_CONTRACT_VERSION}"
        )
    actual_contract = actual["contract"]
    if actual_contract == expected:
        return
    changed = [
        f"{key}={actual_contract.get(key)!r} != {expected.get(key)!r}"
        for key in expected.keys() | actual_contract.keys()
        if actual_contract.get(key) != expected.get(key)
    ]
    raise RuntimeError(f"{label} trace contract changed: {', '.join(sorted(changed))}")


def validate_compact_traces(
    java: str,
    optimized_jar: Path,
    rust_binary: Path,
    main_class: str,
    case: dict,
) -> None:
    expected_trace = case.get("expected_trace")
    if expected_trace is None:
        return
    expected_rating = case["expected_rating"]
    timeout = case.get("timeout_seconds", 60)
    java_trace = parse_compact_trace(
        run_checked(
            java_trace_command(java, optimized_jar, main_class, case),
            case["puzzle"] + "\n",
            timeout,
        ),
        f"java-optimized {case['id']}",
    )
    rust_trace = parse_compact_trace(
        run_checked(rust_trace_command(rust_binary, case), "", timeout),
        f"rust {case['id']}",
    )
    for label, trace in (("java-optimized", java_trace), ("rust", rust_trace)):
        if trace["rating"] != expected_rating:
            raise RuntimeError(
                f"{label} trace rating changed for {case['id']}: "
                f"{trace['rating']} != {expected_rating}"
            )
        require_trace_contract(f"{label} {case['id']}", trace, expected_trace)
    if java_trace["contract"] != rust_trace["contract"]:
        raise RuntimeError(
            f"Java/Rust compact trace mismatch for {case['id']}: "
            f"{java_trace['contract']!r} != {rust_trace['contract']!r}"
        )


def validate_case(
    java: str,
    optimized_jar: Path,
    original_jar: Path,
    rust_binary: Path,
    main_class: str,
    case: dict,
    oracle_case: dict | None,
) -> None:
    expected = case.get("expected_rating")
    if expected is None:
        print(f"warning: {case['id']} has no frozen expected rating; skipping validation")
        return
    payload = case["puzzle"] + "\n"
    timeout = case.get("timeout_seconds", 60)
    actual = last_output_line(
        run_checked(
            java_command(java, optimized_jar, main_class, case, "%r/%p/%d"),
            payload,
            timeout,
        )
    )
    if actual != expected:
        raise RuntimeError(
            f"java-optimized rating changed for {case['id']}: {actual} != {expected}"
        )
    original = last_output_line(
        run_checked(
            java_command(java, original_jar, main_class, case, "%r/%p/%d"),
            payload,
            timeout,
        )
    )
    original_expected = case.get("original_expected_rating")
    if original_expected is None and case["id"] in DEFAULT_CASE_IDS:
        original_expected = expected
    if original_expected is not None and original != original_expected:
        raise RuntimeError(
            f"java-original rating changed for {case['id']}: "
            f"{original} != {original_expected}"
        )
    if original_expected is None:
        print(f"note: java-original rates {case['id']} as {original} (not frozen)")
    fields = run_checked(rust_command(rust_binary, case), payload, timeout).split("\t")
    if len(fields) != 7 or fields[0] != "RESULT":
        raise RuntimeError(f"malformed Rust rating for {case['id']}: {fields!r}")
    actual = "/".join(fields[1:4])
    if actual != expected:
        raise RuntimeError(
            f"rust rating changed for {case['id']}: {actual} != {expected}"
        )
    validate_compact_traces(
        java,
        optimized_jar,
        rust_binary,
        main_class,
        case,
    )
    validate_rust_trace(rust_binary, case, oracle_case)


def validate_rust_trace(rust_binary: Path, case: dict, oracle_case: dict | None) -> None:
    expected = None if oracle_case is None else oracle_case.get("expected")
    if expected is None or "steps" not in expected or "result" not in expected:
        return
    output = run_checked(
        rust_trace_command(rust_binary, case), "", case.get("timeout_seconds", 60)
    )
    steps = []
    result = None
    for line in output.splitlines():
        if line.startswith("STEP\t"):
            fields = line.split("\t", 4)
            if len(fields) != 5:
                raise RuntimeError(f"malformed Rust STEP for {case['id']}: {line}")
            steps.append(
                {
                    "rating": fields[1],
                    "description": fields[2],
                    "grid": fields[3],
                    "candidates": fields[4],
                }
            )
        elif line.startswith("RESULT\t"):
            fields = line.split("\t")
            if len(fields) != 7:
                raise RuntimeError(f"malformed Rust RESULT for {case['id']}: {line}")
            result = {
                "er": fields[1],
                "ep": fields[2],
                "ed": fields[3],
                "er_technique": fields[4],
                "ep_technique": fields[5],
                "ed_technique": fields[6],
            }
    if steps != expected["steps"] or result != expected["result"]:
        raise RuntimeError(
            f"Rust trace/state digest changed for {case['id']}: "
            f"steps={len(steps)}/{len(expected['steps'])}, result={result!r}"
        )


def time_command(command: list[str], payload: str, timeout: int, runs: int) -> list[float]:
    elapsed = []
    for _ in range(runs):
        started = time.perf_counter()
        try:
            completed = subprocess.run(
                command,
                input=payload,
                text=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired as error:
            raise RuntimeError(
                f"benchmark timed out after {timeout}s ({' '.join(command)})"
            ) from error
        elapsed.append(time.perf_counter() - started)
        if completed.returncode != 0:
            raise RuntimeError(
                f"benchmark failed ({' '.join(command)}): {completed.stderr.strip()}"
            )
    return elapsed


def time_command_with_output(
    command: list[str], payload: str, timeout: int
) -> tuple[float, str]:
    """Run one timed process while retaining the output as its rating oracle."""
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command,
            input=payload,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(
            f"benchmark timed out after {timeout}s ({' '.join(command)})"
        ) from error
    elapsed = time.perf_counter() - started
    if completed.returncode != 0:
        raise RuntimeError(
            f"benchmark failed ({' '.join(command)}): {completed.stderr.strip()}"
        )
    return elapsed, completed.stdout


def version(command: list[str]) -> str:
    completed = subprocess.run(command, text=True, capture_output=True, check=False)
    lines = (completed.stdout or completed.stderr).splitlines()
    return lines[0] if lines else "unknown"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_pg_metadata(path: Path = PG_METADATA) -> dict:
    metadata = json.loads(path.read_text(encoding="utf-8"))
    required = {"repository", "commit", "main_class", "jar_sha256", "jar_size"}
    missing = sorted(required - metadata.keys())
    if missing:
        raise RuntimeError(f"PGExplainer metadata is missing: {', '.join(missing)}")
    return metadata


def load_sudokumonster_v118_metadata(
    path: Path = SUDOKUMONSTER_V118_METADATA,
) -> dict:
    metadata = json.loads(path.read_text(encoding="utf-8"))
    required = {
        "repository",
        "tag_baseline",
        "commit",
        "main_class",
        "jar_sha256",
        "jar_size",
    }
    missing = sorted(required - metadata.keys())
    if missing:
        raise RuntimeError(
            f"SudokuMonster v1.18.1 metadata is missing: {', '.join(missing)}"
        )
    return metadata


def require_sudokumonster_v118_artifact(path: Path, metadata: dict) -> None:
    if not path.is_file():
        raise FileNotFoundError(
            "SudokuMonster v1.18.1 artifact not found: "
            f"{path}; run make fetch-sudokumonster-v118"
        )
    actual_size = path.stat().st_size
    if actual_size != metadata["jar_size"]:
        raise RuntimeError(
            "SudokuMonster v1.18.1 JAR size changed: "
            f"{actual_size} != {metadata['jar_size']}"
        )
    actual_hash = sha256(path)
    if actual_hash != metadata["jar_sha256"]:
        raise RuntimeError(
            "SudokuMonster v1.18.1 JAR does not match the pinned release: "
            f"{actual_hash} != {metadata['jar_sha256']}"
        )


def require_pg_artifact(path: Path, metadata: dict) -> None:
    if not path.is_file():
        raise FileNotFoundError(
            f"PGExplainer artifact not found: {path}; run make build-pgexplainer"
        )
    actual_size = path.stat().st_size
    if actual_size != metadata["jar_size"]:
        raise RuntimeError(
            f"PGExplainer JAR size changed: {actual_size} != {metadata['jar_size']}"
        )
    actual_hash = sha256(path)
    if actual_hash != metadata["jar_sha256"]:
        raise RuntimeError(
            "PGExplainer JAR does not match the pinned reproducible build: "
            f"{actual_hash} != {metadata['jar_sha256']}"
        )


def validate_pg_rating(
    java: str,
    jar: Path,
    main_class: str,
    case: dict,
    timeout: int,
) -> str:
    expected = case.get("expected_rating")
    output = run_checked(
        pg_command(java, jar, main_class, "%r/%p/%d"),
        case["puzzle"] + "\n",
        timeout,
    )
    actual = parse_java_rating(output, f"{PG_LABEL} {case['id']}")
    if expected is not None and actual != expected:
        raise RuntimeError(
            f"{PG_LABEL} rates {case['id']} differently: {actual} != {expected}"
        )
    return actual


def main() -> int:
    arguments = parse_arguments()
    if (
        arguments.runs < 1
        or (arguments.copies is not None and arguments.copies < 1)
        or (arguments.pg_timeout is not None and arguments.pg_timeout < 1)
    ):
        raise ValueError("--runs, --copies and --pg-timeout must be positive")
    selected_ids = arguments.case_ids or list(DEFAULT_CASE_IDS)
    document, cases = load_cases(arguments.cases, selected_ids)
    oracle_document = json.loads(arguments.oracle.read_text(encoding="utf-8"))
    oracle_cases = {case["id"]: case for case in oracle_document["cases"]}
    for case in cases:
        require_major_milestone_policy(
            case,
            case["id"] in (arguments.case_ids or []),
            arguments.allow_major_milestone,
            arguments.runs,
            effective_copies(case, arguments.copies),
        )

    optimized_jar = arguments.optimized_jar.resolve()
    original_jar = arguments.original_jar.resolve()
    rust_binary = arguments.rust_binary.resolve()
    for artifact in (optimized_jar, rust_binary):
        if not artifact.is_file():
            raise FileNotFoundError(f"artifact not found: {artifact}")
    original_metadata = load_sudokumonster_v118_metadata()
    require_sudokumonster_v118_artifact(original_jar, original_metadata)

    pg_metadata = None
    pg_jar = arguments.pg_jar.resolve()
    if not arguments.without_pg:
        pg_metadata = load_pg_metadata()
        if any(pg_supported(case) for case in cases):
            require_pg_artifact(pg_jar, pg_metadata)

    print(version([arguments.java, "-version"]))
    print(version([str(rust_binary), "--version"]))
    print(
        "SudokuMonster SukakuExplainer "
        f"{original_metadata['tag_baseline']} ({original_metadata['commit']})"
    )
    if pg_metadata is not None:
        print(
            "PGExplainer "
            f"{pg_metadata['commit']} (as shipped: classic-only, multithreaded)"
        )
    print(f"runs={arguments.runs}; each run starts a fresh process")
    print()
    print(f"{'case':38} {'engine':22} {'copies':>6} {'median':>9} {'ms/puzzle':>11}")
    print("-" * 92)
    for case in cases:
        copies = effective_copies(case, arguments.copies)
        payload = (case["puzzle"] + "\n") * copies
        timeout = case.get("timeout_seconds", 60)
        if uses_single_solve_rating_path(case):
            engines = (
                (
                    "java-original",
                    java_command(
                        arguments.java,
                        original_jar,
                        document["main_class"],
                        case,
                        "%r/%p/%d",
                    ),
                    parse_java_rating,
                ),
                (
                    "java-optimized",
                    java_command(
                        arguments.java,
                        optimized_jar,
                        document["main_class"],
                        case,
                        "%r/%p/%d",
                    ),
                    parse_java_rating,
                ),
                (
                    "rust-release",
                    rust_command(rust_binary, case),
                    parse_rust_rating,
                ),
            )
            timings: list[tuple[str, float]] = []
            ratings: dict[str, str] = {}
            for label, command, parse_rating in engines:
                elapsed, output = time_command_with_output(command, payload, timeout)
                timings.append((label, elapsed))
                ratings[label] = parse_rating(output, f"{label} {case['id']}")
            rating = require_rating_consensus(case["id"], ratings)
            require_frozen_major_milestone_rating(case, rating)
            for label, elapsed in timings:
                print(
                    f"{case['id']:38} {label:22} {copies:6d} {elapsed:8.3f}s "
                    f"{elapsed * 1000.0 / copies:11.3f}"
                )
            if pg_metadata is not None and pg_supported(case):
                pg_copies = effective_pg_copies(case, arguments.copies)
                pg_payload = (case["puzzle"] + "\n") * pg_copies
                pg_elapsed, pg_output = time_command_with_output(
                    pg_command(
                        arguments.java,
                        pg_jar,
                        pg_metadata["main_class"],
                        "%r/%p/%d",
                    ),
                    pg_payload,
                    effective_pg_timeout(case, arguments.pg_timeout),
                )
                pg_rating = parse_java_rating(
                    pg_output, f"{PG_LABEL} {case['id']}"
                )
                require_frozen_major_milestone_rating(case, pg_rating)
                print(
                    f"{case['id']:38} {PG_LABEL:22} {pg_copies:6d} "
                    f"{pg_elapsed:8.3f}s "
                    f"{pg_elapsed * 1000.0 / pg_copies:11.3f}"
                )
            contract = (
                "unfrozen rating"
                if case.get("expected_rating") is None
                else "frozen rating"
            )
            print(f"{case['id']} {contract} (all engines agree): {rating}")
            continue

        validate_case(
            arguments.java,
            optimized_jar,
            original_jar,
            rust_binary,
            document["main_class"],
            case,
            oracle_cases.get(case["id"]),
        )
        engine_specs = [
            (
                "java-original",
                java_command(
                    arguments.java,
                    original_jar,
                    document["main_class"],
                    case,
                    "",
                ),
                copies,
                timeout,
            ),
            (
                "java-optimized",
                java_command(
                    arguments.java,
                    optimized_jar,
                    document["main_class"],
                    case,
                    "",
                ),
                copies,
                timeout,
            ),
            (
                "rust-release",
                rust_command(rust_binary, case, quiet=True),
                copies,
                timeout,
            ),
        ]
        pg_skipped = False
        if pg_metadata is not None:
            if pg_supported(case):
                pg_timeout = effective_pg_timeout(case, arguments.pg_timeout)
                validate_pg_rating(
                    arguments.java,
                    pg_jar,
                    pg_metadata["main_class"],
                    case,
                    pg_timeout,
                )
                engine_specs.append(
                    (
                        PG_LABEL,
                        pg_command(
                            arguments.java,
                            pg_jar,
                            pg_metadata["main_class"],
                            "",
                        ),
                        effective_pg_copies(case, arguments.copies),
                        pg_timeout,
                    )
                )
            else:
                pg_skipped = True
        for label, command, engine_copies, engine_timeout in engine_specs:
            engine_payload = (case["puzzle"] + "\n") * engine_copies
            elapsed = time_command(
                command,
                engine_payload,
                engine_timeout,
                arguments.runs,
            )
            median = statistics.median(elapsed)
            print(
                f"{case['id']:38} {label:22} {engine_copies:6d} {median:8.3f}s "
                f"{median * 1000.0 / engine_copies:11.3f}"
            )
        if pg_skipped:
            print(
                f"{case['id']:38} {PG_LABEL:22} {'SKIP':>6} "
                f"{'variant unsupported':>21}"
            )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (FileNotFoundError, OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
