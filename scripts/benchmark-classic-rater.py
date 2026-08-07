#!/usr/bin/env python3
"""Benchmark the corrected Classic rater against explicit rating engines.

The harness is intentionally independent of the old sibling SukakuExplainer
checkout. Every measured run starts a fresh process and retains its output.
The headless rater must preserve its frozen ER/EP/ED result; updated Java and
other explicitly unfrozen comparators may report a different stable rating.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import signal
import statistics
import subprocess
import tempfile
import time
from typing import Any, Callable, Sequence


ROOT = Path(__file__).resolve().parents[1]
SE121_METADATA = Path(__file__).with_name("se121-oracle.json")
SUDOKUMONSTER_V118_METADATA = Path(__file__).with_name("sudokumonster-v118.json")
PG_METADATA = Path(__file__).with_name("pgexplainer.json")
DEFAULT_SE121_JAR = ROOT / "target" / "se121-oracle" / "serate.jar"
DEFAULT_SUDOKUMONSTER_V118_JAR = (
    ROOT / "target" / "sudokumonster" / "SukakuExplainer-v1.18.1.jar"
)
DEFAULT_PG_JAR = ROOT / "target" / "pgexplainer" / "PGExplainer.jar"
PROTECTED_CASE_ID = "user_extreme_major_milestone_probe"
RATING_PATTERN = re.compile(r"[0-9]+\.[0-9]/[0-9]+\.[0-9]/[0-9]+\.[0-9]")

CASES: tuple[dict[str, Any], ...] = (
    {
        "id": "static_forcing_chain_7_2",
        "puzzle": ".3...89.2..2....4.......567...76...34...53........485.96..3....28.41.6.......617.",
        "expected_rating": "7.2/1.2/1.2",
    },
    {
        "id": "aligned_pair_and_dynamic_8_9",
        "puzzle": "100000002520070049009000500000689000000703000090105030640010025010000070900000008",
        "expected_rating": "8.9/1.5/1.5",
    },
    {
        "id": "dynamic_forcing_chain_plus_9_3",
        "puzzle": "300205000000000010008060200000007604009300000600080000000000920500104006070000005",
        "expected_rating": "9.3/1.2/1.2",
    },
    {
        "id": "dynamic_forcing_chain_plus_9_8",
        "puzzle": "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
        "expected_rating": "9.8/9.8/9.5",
    },
    {
        "id": "ai_escargot_nested_10_5",
        "puzzle": "1....7.9..3..2...8..96..5....53..9...1..8...26....4...3......1..4......7..7...3..",
        "expected_rating": "10.5/1.2/1.2",
    },
    {
        "id": PROTECTED_CASE_ID,
        "puzzle": "98.7..6....5.4...........9.8..9...6..4..5...9..9..32..1.........7.1...8...8..2..3",
        "expected_rating": "11.8/1.2/1.2",
        "major_milestone_only": True,
    },
)
DEFAULT_CASE_IDS = (
    "dynamic_forcing_chain_plus_9_3",
    "dynamic_forcing_chain_plus_9_8",
)


class BenchmarkError(RuntimeError):
    """A benchmark precondition, subprocess, or rating check failed."""


class BenchmarkTimeout(BenchmarkError):
    """A benchmark process exceeded its configured wall-clock limit."""

    def __init__(self, command: list[str], timeout: float, elapsed: float) -> None:
        super().__init__(
            f"benchmark timed out after {timeout:.3f}s ({' '.join(command)})"
        )
        self.timeout = timeout
        self.elapsed = elapsed


CommandFactory = Callable[[Path], list[str]]


class Engine:
    def __init__(
        self,
        label: str,
        command_factory: CommandFactory,
        artifact: Path | None,
        version: str,
        pinned: bool = True,
        enforce_frozen_rating: bool = True,
        thread_policy: str = "single-threaded",
    ) -> None:
        self.label = label
        self.command_factory = command_factory
        self.artifact = artifact
        self.version = version
        self.pinned = pinned
        self.enforce_frozen_rating = enforce_frozen_rating
        self.thread_policy = thread_policy


def parse_arguments(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--case",
        action="append",
        dest="case_ids",
        choices=[case["id"] for case in CASES],
        help="select a named case; may be repeated",
    )
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--copies", type=int, default=1)
    parser.add_argument(
        "--warmup",
        type=int,
        default=1,
        help="untimed fresh-process runs before measurement",
    )
    parser.add_argument(
        "--allow-major-milestone",
        action="store_true",
        help="explicitly authorize the protected 11.8 one-shot benchmark",
    )
    parser.add_argument(
        "--pre-rater",
        type=Path,
        help="pre-optimization sukaku-forge-rate binary",
    )
    parser.add_argument(
        "--post-rater",
        type=Path,
        help="post-optimization sukaku-forge-rate binary",
    )
    parser.add_argument(
        "--rater-mode",
        choices=("default", "uniqueness", "both"),
        default="default",
        help="benchmark the product default, compatibility opt-in, or both",
    )
    parser.add_argument(
        "--generic-rust",
        type=Path,
        help="generic sukaku-forge binary (uses batch-rate)",
    )
    parser.add_argument(
        "--se121-jar",
        type=Path,
        nargs="?",
        const=DEFAULT_SE121_JAR,
        help="include the pinned SE 1.2.1 source oracle",
    )
    parser.add_argument(
        "--sudokumonster-v118-jar",
        type=Path,
        nargs="?",
        const=DEFAULT_SUDOKUMONSTER_V118_JAR,
        help="include the pinned SudokuMonster v1.18.1 release comparator",
    )
    parser.add_argument(
        "--pg-jar",
        type=Path,
        nargs="?",
        const=DEFAULT_PG_JAR,
        help="include the pinned PGExplainer comparator",
    )
    parser.add_argument(
        "--engine",
        action="append",
        default=[],
        metavar="LABEL=COMMAND",
        help=(
            "add a command that reads puzzles from stdin and emits one rating "
            "or Forge RESULT per puzzle; shell syntax is parsed but no shell runs"
        ),
    )
    parser.add_argument("--java", default=os.environ.get("SE121_JAVA", "java"))
    parser.add_argument("--timeout", type=float, default=600.0)
    parser.add_argument(
        "--engine-timeout",
        action="append",
        default=[],
        metavar="LABEL=SECONDS",
        help="override the timeout for one engine",
    )
    parser.add_argument(
        "--allow-timeout",
        action="append",
        default=[],
        metavar="LABEL",
        help="record one engine's timeout as a censored benchmark result",
    )
    parser.add_argument(
        "--cpu",
        type=int,
        help="logical CPU used by taskset (default: highest available CPU)",
    )
    parser.add_argument(
        "--no-affinity",
        action="store_true",
        help="do not pin benchmark processes to one logical CPU",
    )
    parser.add_argument(
        "--unpin-engine",
        action="append",
        default=[],
        metavar="LABEL",
        help="let one engine use the full inherited CPU set (for PG as shipped)",
    )
    parser.add_argument(
        "--json-out",
        type=Path,
        help="also write complete metadata and measurements as JSON",
    )
    return parser.parse_args(argv)


def selected_cases(case_ids: list[str] | None) -> list[dict[str, Any]]:
    selected = list(DEFAULT_CASE_IDS if case_ids is None else case_ids)
    by_id = {case["id"]: case for case in CASES}
    return [by_id[identifier] for identifier in selected]


def require_benchmark_policy(arguments: argparse.Namespace, cases: list[dict]) -> None:
    if arguments.runs < 1 or arguments.copies < 1 or arguments.warmup < 0:
        raise ValueError("--runs and --copies must be positive; --warmup cannot be negative")
    if arguments.timeout <= 0:
        raise ValueError("--timeout must be positive")
    protected_selected = any(case.get("major_milestone_only") for case in cases)
    if not protected_selected:
        if arguments.allow_major_milestone:
            raise ValueError(
                "--allow-major-milestone is valid only with the exact protected case"
            )
        return
    exact_selection = arguments.case_ids == [PROTECTED_CASE_ID]
    if not exact_selection or not arguments.allow_major_milestone:
        raise ValueError(
            f"{PROTECTED_CASE_ID} requires exact --case plus "
            "--allow-major-milestone"
        )
    if arguments.runs != 1:
        raise ValueError(f"{PROTECTED_CASE_ID} requires --runs 1")
    if arguments.copies != 1:
        raise ValueError(f"{PROTECTED_CASE_ID} requires --copies 1")
    if arguments.warmup != 0:
        raise ValueError(f"{PROTECTED_CASE_ID} requires --warmup 0")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_metadata(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise BenchmarkError(f"cannot load {path}: {error}") from error
    if not isinstance(value, dict):
        raise BenchmarkError(f"{path} must contain an object")
    return value


def require_artifact(path: Path, metadata: dict[str, Any], label: str) -> Path:
    resolved = path.resolve()
    if not resolved.is_file():
        raise FileNotFoundError(f"{label} artifact not found: {resolved}")
    expected_size = metadata.get("jar_size")
    if resolved.stat().st_size != expected_size:
        raise BenchmarkError(
            f"{label} JAR size changed: {resolved.stat().st_size} != {expected_size}"
        )
    actual_hash = sha256(resolved)
    expected_hash = metadata.get("jar_sha256")
    if actual_hash != expected_hash:
        raise BenchmarkError(
            f"{label} JAR SHA-256 changed: {actual_hash} != {expected_hash}"
        )
    return resolved


def run_identification(command: list[str]) -> str:
    try:
        result = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=10.0,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise BenchmarkError(f"cannot identify {' '.join(command)}: {error}") from error
    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if result.returncode != 0 or not lines:
        raise BenchmarkError(f"cannot identify {' '.join(command)}")
    return " | ".join(lines)


def require_java_runtime(java: str, metadata: dict[str, Any]) -> str:
    try:
        result = subprocess.run(
            [java, "-version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=10.0,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise BenchmarkError(f"cannot identify Java runtime: {error}") from error
    if result.returncode != 0:
        raise BenchmarkError(f"java -version exited with {result.returncode}")
    expected = metadata.get("java_version_output_sha256")
    if expected is not None:
        actual = hashlib.sha256(result.stdout).hexdigest()
        if actual != expected:
            raise BenchmarkError(
                f"Java runtime fingerprint changed: {actual} != {expected}"
            )
    return " | ".join(
        line.strip()
        for line in result.stdout.decode("utf-8", errors="replace").splitlines()
        if line.strip()
    )


def executable_engine(
    label: str,
    path: Path,
    arguments: list[str],
) -> Engine:
    resolved = path.resolve()
    if not resolved.is_file():
        raise FileNotFoundError(f"{label} executable not found: {resolved}")
    return Engine(
        label,
        lambda _preferences, binary=resolved, suffix=tuple(arguments): [
            str(binary),
            *suffix,
        ],
        resolved,
        run_identification([str(resolved), "--version"]),
    )


def custom_engine(specification: str) -> Engine:
    if "=" not in specification:
        raise ValueError("--engine must be LABEL=COMMAND")
    label, text_command = specification.split("=", 1)
    label = label.strip()
    command = shlex.split(text_command)
    if not label or not command:
        raise ValueError("--engine must contain a nonempty label and command")
    artifact = Path(command[0])
    resolved_artifact = artifact.resolve() if artifact.is_file() else None
    return Engine(
        label,
        lambda _preferences, value=tuple(command): list(value),
        resolved_artifact,
        "custom command (version not probed)",
        pinned=False,
        enforce_frozen_rating=False,
        thread_policy="unknown custom command",
    )


def build_engines(
    arguments: argparse.Namespace, preferences_root: Path
) -> list[Engine]:
    del preferences_root  # command factories receive the per-invocation directory
    engines: list[Engine] = []
    modes = (
        ("default", []),
        ("uniqueness", ["--allow-uniqueness"]),
    )
    enabled_modes = {
        "default": {"default"},
        "uniqueness": {"uniqueness"},
        "both": {"default", "uniqueness"},
    }[arguments.rater_mode]
    for prefix, path in (("pre", arguments.pre_rater), ("post", arguments.post_rater)):
        if path is None:
            continue
        for mode, mode_arguments in modes:
            if mode in enabled_modes:
                engines.append(
                    executable_engine(
                        f"{prefix}-{mode}",
                        path,
                        ["--format=%r/%p/%d", "--input=-", *mode_arguments],
                    )
                )
    if arguments.generic_rust is not None:
        engine = executable_engine(
            "rust-generic-v118", arguments.generic_rust, ["batch-rate"]
        )
        engine.enforce_frozen_rating = False
        engines.append(engine)
    if arguments.se121_jar is not None:
        metadata = load_metadata(SE121_METADATA)
        jar = require_artifact(arguments.se121_jar, metadata, "SE 1.2.1 oracle")
        java_version = require_java_runtime(arguments.java, metadata)
        main_class = metadata["main_class"]
        engines.append(
            Engine(
                "java-se121-oracle",
                lambda preferences, java=arguments.java, artifact=jar, main=main_class: [
                    java,
                    "-Xrs",
                    "-Xmx500m",
                    f"-Djava.util.prefs.userRoot={preferences}",
                    "-cp",
                    str(artifact),
                    main,
                    "--format=%r/%p/%d",
                    "--input=-",
                ],
                jar,
                f"{java_version}; SE121 commit {metadata['commit']}",
            )
        )
    if arguments.sudokumonster_v118_jar is not None:
        metadata = load_metadata(SUDOKUMONSTER_V118_METADATA)
        jar = require_artifact(
            arguments.sudokumonster_v118_jar,
            metadata,
            "SudokuMonster v1.18.1",
        )
        java_version = require_java_runtime(arguments.java, metadata)
        main_class = metadata["main_class"]
        engines.append(
            Engine(
                "java-sudokumonster-v118",
                lambda preferences, java=arguments.java, artifact=jar, main=main_class: [
                    java,
                    "-Xrs",
                    "-Xmx500m",
                    f"-Djava.util.prefs.userRoot={preferences}",
                    "-cp",
                    str(artifact),
                    main,
                    "--threads=1",
                    "--format=%r/%p/%d",
                    "--input=-",
                ],
                jar,
                (
                    f"{java_version}; SudokuMonster {metadata['tag_baseline']} "
                    f"commit {metadata['commit']}"
                ),
                enforce_frozen_rating=False,
                thread_policy="single-threaded (--threads=1)",
            )
        )
    if arguments.pg_jar is not None:
        metadata = load_metadata(PG_METADATA)
        jar = require_artifact(arguments.pg_jar, metadata, "PGExplainer")
        java_version = run_identification([arguments.java, "-version"])
        main_class = metadata["main_class"]
        engines.append(
            Engine(
                "pgexplainer",
                lambda _preferences, java=arguments.java, artifact=jar, main=main_class: [
                    java,
                    "-Xrs",
                    "-Xmx500m",
                    "-cp",
                    str(artifact),
                    main,
                    "--format=%r/%p/%d",
                    "--input=-",
                ],
                jar,
                f"{java_version}; PG commit {metadata['commit']}",
                enforce_frozen_rating=False,
                thread_policy="upstream worker threads constrained only by affinity",
            )
        )
    engines.extend(custom_engine(specification) for specification in arguments.engine)
    if not engines:
        raise ValueError("select at least one benchmark engine")
    labels = [engine.label for engine in engines]
    duplicates = sorted({label for label in labels if labels.count(label) > 1})
    if duplicates:
        raise ValueError(f"duplicate engine label(s): {', '.join(duplicates)}")
    return engines


def parse_cpu_list(value: str) -> set[int]:
    cpus: set[int] = set()
    for item in value.strip().split(","):
        if not item:
            continue
        if "-" in item:
            start, end = item.split("-", 1)
            cpus.update(range(int(start), int(end) + 1))
        else:
            cpus.add(int(item))
    return cpus


def available_cpus() -> list[int]:
    try:
        cpus = set(os.sched_getaffinity(0))
    except AttributeError:
        cpus = set(range(os.cpu_count() or 1))
    cpuset_path = Path("/sys/fs/cgroup/cpuset.cpus.effective")
    try:
        effective = parse_cpu_list(cpuset_path.read_text(encoding="ascii"))
    except (OSError, ValueError):
        effective = set()
    if effective:
        cpus &= effective
    return sorted(cpus)


def affinity_policy(arguments: argparse.Namespace) -> tuple[str | None, int | None]:
    if arguments.no_affinity:
        return None, None
    taskset = shutil.which("taskset")
    if taskset is None:
        return None, None
    cpus = available_cpus()
    if not cpus:
        raise BenchmarkError("taskset is available but no permitted CPU was found")
    cpu = cpus[-1] if arguments.cpu is None else arguments.cpu
    if cpu not in cpus:
        raise ValueError(f"--cpu {cpu} is not in the available CPU set {cpus}")
    return taskset, cpu


def parse_engine_timeouts(values: list[str], labels: set[str]) -> dict[str, float]:
    result: dict[str, float] = {}
    for value in values:
        if "=" not in value:
            raise ValueError("--engine-timeout must be LABEL=SECONDS")
        label, seconds_text = value.split("=", 1)
        try:
            seconds = float(seconds_text)
        except ValueError as error:
            raise ValueError(f"invalid timeout {seconds_text!r}") from error
        if label not in labels:
            raise ValueError(f"timeout names unknown engine {label!r}")
        if seconds <= 0:
            raise ValueError("engine timeouts must be positive")
        result[label] = seconds
    return result


def terminate_process_group(process: subprocess.Popen[str]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=2.0)
    except ProcessLookupError:
        return
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            return
        process.wait()


def close_process_pipes(process: subprocess.Popen[str]) -> None:
    for stream in (process.stdin, process.stdout, process.stderr):
        if stream is not None and not stream.closed:
            stream.close()


def run_process(command: list[str], payload: str, timeout: float) -> tuple[float, str]:
    started = time.perf_counter()
    try:
        process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
    except OSError as error:
        raise BenchmarkError(f"cannot start {' '.join(command)}: {error}") from error
    try:
        stdout, stderr = process.communicate(payload, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        terminate_process_group(process)
        close_process_pipes(process)
        elapsed = time.perf_counter() - started
        raise BenchmarkTimeout(command, timeout, elapsed) from error
    elapsed = time.perf_counter() - started
    if process.returncode != 0:
        detail = stderr.strip() or stdout.strip() or "no diagnostics"
        raise BenchmarkError(
            f"benchmark exited with {process.returncode} ({' '.join(command)}): {detail}"
        )
    return elapsed, stdout


def parse_ratings(output: str, expected_lines: int, label: str) -> list[str]:
    ratings: list[str] = []
    for line in (line.strip() for line in output.splitlines() if line.strip()):
        if RATING_PATTERN.fullmatch(line):
            ratings.append(line)
            continue
        if line.startswith("RESULT\t"):
            fields = line.split("\t")
            if len(fields) != 7:
                raise BenchmarkError(f"malformed {label} RESULT: {line!r}")
            rating = "/".join(fields[1:4])
            if RATING_PATTERN.fullmatch(rating) is None:
                raise BenchmarkError(f"malformed {label} rating: {rating!r}")
            ratings.append(rating)
            continue
        raise BenchmarkError(f"unexpected {label} output: {line!r}")
    if len(ratings) != expected_lines:
        raise BenchmarkError(
            f"{label} emitted {len(ratings)} ratings, expected {expected_lines}"
        )
    return ratings


def cpu_description() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
            if line.lower().startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def machine_metadata(taskset: str | None, cpu: int | None) -> dict[str, Any]:
    try:
        cpu_quota = Path("/sys/fs/cgroup/cpu.max").read_text(encoding="ascii").strip()
    except OSError:
        cpu_quota = "unknown"
    return {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "cpu_model": cpu_description(),
        "logical_cpus": os.cpu_count(),
        "available_cpus": available_cpus(),
        "cgroup_cpu_max": cpu_quota,
        "affinity": "unrestricted" if taskset is None else f"single CPU {cpu}",
        "load_average_start": os.getloadavg(),
    }


def benchmark(
    arguments: argparse.Namespace,
    cases: list[dict[str, Any]],
    engines: list[Engine],
    preferences_root: Path,
) -> dict[str, Any]:
    taskset, cpu = affinity_policy(arguments)
    labels = {engine.label for engine in engines}
    unknown_unpinned = sorted(set(arguments.unpin_engine) - labels)
    if unknown_unpinned:
        raise ValueError(f"unknown --unpin-engine label(s): {', '.join(unknown_unpinned)}")
    unknown_timeout_labels = sorted(set(arguments.allow_timeout) - labels)
    if unknown_timeout_labels:
        raise ValueError(
            f"unknown --allow-timeout label(s): {', '.join(unknown_timeout_labels)}"
        )
    timeouts = parse_engine_timeouts(arguments.engine_timeout, labels)
    machine = machine_metadata(taskset, cpu)
    engine_metadata = []
    for engine in engines:
        artifact_hash = sha256(engine.artifact) if engine.artifact is not None else None
        command = engine.command_factory(Path("<fresh-java-prefs>"))
        if taskset is not None and engine.label not in arguments.unpin_engine:
            command = [taskset, "-c", str(cpu), *command]
        engine_metadata.append(
            {
                "label": engine.label,
                "version": engine.version,
                "artifact": str(engine.artifact) if engine.artifact is not None else None,
                "sha256": artifact_hash,
                "command": shlex.join(command),
                "pinned_artifact": engine.pinned,
                "thread_policy": engine.thread_policy,
                "rating_contract": (
                    "frozen SE121-derived rating"
                    if engine.enforce_frozen_rating
                    else "stable observed comparator rating"
                ),
                "affinity": (
                    "unrestricted"
                    if taskset is None or engine.label in arguments.unpin_engine
                    else f"single CPU {cpu}"
                ),
            }
        )

    print(f"machine: {machine['cpu_model']}")
    print(
        f"logical CPUs: {machine['logical_cpus']}; available: "
        f"{machine['available_cpus']}; default affinity: {machine['affinity']}"
    )
    print(
        f"policy: runs={arguments.runs}, copies={arguments.copies}, "
        f"warmup={arguments.warmup}; every run is a fresh process"
    )
    for item in engine_metadata:
        print(
            f"engine {item['label']}: {item['version']}; "
            f"sha256={item['sha256'] or 'unavailable'}; affinity={item['affinity']}; "
            f"threads={item['thread_policy']}; rating={item['rating_contract']}"
        )
        print(f"  command: {item['command']}")
    print()
    print(f"{'case':38} {'engine':24} {'rating':14} {'median':>10} {'min':>10} {'max':>10}")
    print("-" * 112)

    measurements: list[dict[str, Any]] = []
    invocation = 0
    for case in cases:
        payload = (case["puzzle"] + "\n") * arguments.copies
        for engine in engines:
            timeout = timeouts.get(engine.label, arguments.timeout)
            elapsed_values: list[float] = []
            observed_rating: str | None = None
            timed_out = False
            timeout_elapsed: float | None = None
            total_runs = arguments.warmup + arguments.runs
            for run_index in range(total_runs):
                invocation += 1
                preferences = preferences_root / f"invocation-{invocation}"
                preferences.mkdir()
                command = engine.command_factory(preferences)
                if taskset is not None and engine.label not in arguments.unpin_engine:
                    command = [taskset, "-c", str(cpu), *command]
                try:
                    elapsed, output = run_process(command, payload, timeout)
                except BenchmarkTimeout as error:
                    if engine.label not in arguments.allow_timeout:
                        raise
                    timed_out = True
                    timeout_elapsed = error.elapsed
                    break
                ratings = parse_ratings(
                    output,
                    arguments.copies,
                    f"{engine.label} {case['id']}",
                )
                expected = case["expected_rating"]
                distinct_ratings = set(ratings)
                if len(distinct_ratings) != 1:
                    raise BenchmarkError(
                        f"{engine.label} returned inconsistent ratings within one "
                        f"batch for {case['id']}: {ratings!r}"
                    )
                run_rating = ratings[0]
                if engine.enforce_frozen_rating and run_rating != expected:
                    raise BenchmarkError(
                        f"{engine.label} changed the frozen rating for {case['id']}: "
                        f"{run_rating} != {expected}"
                    )
                if observed_rating is None:
                    observed_rating = run_rating
                elif run_rating != observed_rating:
                    raise BenchmarkError(
                        f"{engine.label} rating was not repeatable for {case['id']}: "
                        f"{run_rating} != {observed_rating}"
                    )
                if run_index >= arguments.warmup:
                    elapsed_values.append(elapsed)
            if timed_out:
                print(
                    f"{case['id']:38} {engine.label:24} {'TIMEOUT':14} "
                    f">{timeout:8.3f}s {'-':>10} {'-':>10}"
                )
                measurements.append(
                    {
                        "case": case["id"],
                        "engine": engine.label,
                        "rating": None,
                        "copies": arguments.copies,
                        "warmup_runs": arguments.warmup,
                        "elapsed_seconds": elapsed_values,
                        "seconds_per_puzzle": [
                            elapsed / arguments.copies for elapsed in elapsed_values
                        ],
                        "timed_out": True,
                        "timeout_seconds": timeout,
                        "termination_elapsed_seconds": timeout_elapsed,
                        "output_validated": False,
                    }
                )
                continue
            median = statistics.median(elapsed_values)
            minimum = min(elapsed_values)
            maximum = max(elapsed_values)
            per_puzzle = [elapsed / arguments.copies for elapsed in elapsed_values]
            print(
                f"{case['id']:38} {engine.label:24} {observed_rating:14} "
                f"{median:9.3f}s {minimum:9.3f}s {maximum:9.3f}s"
            )
            measurements.append(
                {
                    "case": case["id"],
                    "engine": engine.label,
                    "rating": observed_rating,
                    "matches_se121_derived_rating": (
                        observed_rating == case["expected_rating"]
                    ),
                    "copies": arguments.copies,
                    "warmup_runs": arguments.warmup,
                    "elapsed_seconds": elapsed_values,
                    "seconds_per_puzzle": per_puzzle,
                    "median_seconds": median,
                    "median_seconds_per_puzzle": statistics.median(per_puzzle),
                    "timeout_seconds": timeout,
                    "timed_out": False,
                    "output_validated": True,
                }
            )
    by_case_engine = {
        (measurement["case"], measurement["engine"]): measurement
        for measurement in measurements
    }
    comparisons: list[dict[str, Any]] = []
    for case in cases:
        for mode in ("default", "uniqueness"):
            before = by_case_engine.get((case["id"], f"pre-{mode}"))
            after = by_case_engine.get((case["id"], f"post-{mode}"))
            if (
                before is None
                or after is None
                or before.get("timed_out")
                or after.get("timed_out")
            ):
                continue
            speedup = before["median_seconds"] / after["median_seconds"]
            reduction = 1.0 - after["median_seconds"] / before["median_seconds"]
            comparison = {
                "case": case["id"],
                "mode": mode,
                "pre_engine": f"pre-{mode}",
                "post_engine": f"post-{mode}",
                "speedup": speedup,
                "wall_time_reduction_fraction": reduction,
            }
            comparisons.append(comparison)
            print(
                f"speedup {case['id']} ({mode}): {speedup:.3f}x; "
                f"wall-time reduction {reduction * 100.0:.1f}%"
            )
    machine["load_average_end"] = os.getloadavg()
    return {
        "schema_version": 1,
        "generated_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "machine": machine,
        "policy": {
            "runs": arguments.runs,
            "copies": arguments.copies,
            "warmup": arguments.warmup,
            "fresh_process_per_run": True,
            "protected_case": any(case.get("major_milestone_only") for case in cases),
        },
        "engines": engine_metadata,
        "measurements": measurements,
        "pre_post_comparisons": comparisons,
    }


def main(argv: Sequence[str] | None = None) -> int:
    arguments = parse_arguments(argv)
    cases = selected_cases(arguments.case_ids)
    require_benchmark_policy(arguments, cases)
    with tempfile.TemporaryDirectory(prefix="forge-classic-benchmark-") as temp:
        preferences_root = Path(temp)
        engines = build_engines(arguments, preferences_root)
        report = benchmark(arguments, cases, engines, preferences_root)
    if arguments.json_out is not None:
        arguments.json_out.parent.mkdir(parents=True, exist_ok=True)
        arguments.json_out.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(f"wrote {arguments.json_out}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (BenchmarkError, FileNotFoundError, OSError, ValueError) as error:
        raise SystemExit(f"error: {error}") from error
