#!/usr/bin/env python3
"""Rate an ordered slice of the retained public hard-puzzle corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time
from typing import Any, Sequence


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CORPUS = Path(__file__).with_name("hard-puzzle-corpus.json")
DEFAULT_RATER = ROOT / "target" / "rater" / "sukaku-forge-rate"
RATING_PATTERN = re.compile(r"[0-9]+\.[0-9]/[0-9]+\.[0-9]/[0-9]+\.[0-9]")
SHA256_PATTERN = re.compile(r"[0-9a-f]{64}")


class CorpusBenchmarkError(RuntimeError):
    """The corpus, rater, or emitted rating stream is invalid."""


def parse_arguments(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--rater", type=Path, default=DEFAULT_RATER)
    parser.add_argument("--start", type=int, default=1, help="1-based first row")
    parser.add_argument("--limit", type=int, default=10)
    parser.add_argument("--timeout", type=float, default=1800.0)
    parser.add_argument("--allow-uniqueness", action="store_true")
    parser.add_argument("--json-out", type=Path)
    return parser.parse_args(argv)


def load_corpus(path: Path) -> dict[str, Any]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CorpusBenchmarkError(f"cannot load {path}: {error}") from error
    if not isinstance(document, dict):
        raise CorpusBenchmarkError(f"corpus root must be an object in {path}")
    if type(document.get("schema_version")) is not int or document["schema_version"] != 1:
        raise CorpusBenchmarkError(f"unsupported corpus schema in {path}")
    cases = document.get("cases")
    if not isinstance(cases, list):
        raise CorpusBenchmarkError(f"corpus cases must be an array in {path}")
    puzzles: set[str] = set()
    normalized = bytearray()
    for position, case in enumerate(cases, start=1):
        if not isinstance(case, dict) or set(case) != {"expanded_minlex", "rating"}:
            raise CorpusBenchmarkError(f"malformed corpus case {position} in {path}")
        puzzle = case["expanded_minlex"]
        rating = case["rating"]
        if (
            not isinstance(puzzle, str)
            or len(puzzle) != 81
            or not set(puzzle) <= set(".123456789")
            or puzzle in puzzles
        ):
            raise CorpusBenchmarkError(f"invalid or duplicate puzzle at case {position} in {path}")
        if not isinstance(rating, str) or RATING_PATTERN.fullmatch(rating) is None:
            raise CorpusBenchmarkError(f"invalid rating at case {position} in {path}")
        puzzles.add(puzzle)
        normalized.extend(f"{puzzle}\t{rating}\n".encode("ascii"))
    expected_digest = document.get("cases_sha256")
    actual_digest = hashlib.sha256(normalized).hexdigest()
    if (
        not isinstance(expected_digest, str)
        or SHA256_PATTERN.fullmatch(expected_digest) is None
        or expected_digest != actual_digest
    ):
        raise CorpusBenchmarkError(
            f"corpus case digest mismatch in {path}: {actual_digest}"
        )
    return document


def select_cases(
    document: dict[str, Any], start: int, limit: int
) -> list[dict[str, str]]:
    if start < 1 or limit < 1:
        raise ValueError("--start and --limit must be positive")
    cases = document["cases"]
    selected = cases[start - 1 : start - 1 + limit]
    if len(selected) != limit:
        raise ValueError(
            f"requested rows {start}..{start + limit - 1}, corpus has {len(cases)}"
        )
    return selected


def parse_ratings(output: str, expected_count: int) -> list[str]:
    ratings = [line.strip() for line in output.splitlines() if line.strip()]
    if len(ratings) != expected_count:
        raise CorpusBenchmarkError(
            f"rater emitted {len(ratings)} ratings, expected {expected_count}"
        )
    malformed = [rating for rating in ratings if RATING_PATTERN.fullmatch(rating) is None]
    if malformed:
        raise CorpusBenchmarkError(f"malformed rater output: {malformed[0]!r}")
    return ratings


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_benchmark(arguments: argparse.Namespace) -> dict[str, Any]:
    document = load_corpus(arguments.corpus)
    cases = select_cases(document, arguments.start, arguments.limit)
    rater = arguments.rater.resolve()
    if not rater.is_file():
        raise FileNotFoundError(f"rater not found: {rater}")
    command = [str(rater), "--format=%r/%p/%d", "--input=-"]
    if arguments.allow_uniqueness:
        command.append("--allow-uniqueness")
    payload = "".join(case["expanded_minlex"] + "\n" for case in cases)
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command,
            input=payload,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=arguments.timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise CorpusBenchmarkError(
            f"rater exceeded the {arguments.timeout:g}s timeout"
        ) from error
    elapsed = time.perf_counter() - started
    if completed.returncode != 0:
        raise CorpusBenchmarkError(
            f"rater exited {completed.returncode}: {completed.stderr.strip()}"
        )
    actual = parse_ratings(completed.stdout, len(cases))
    results = []
    for offset, (case, rating) in enumerate(zip(cases, actual, strict=True)):
        results.append(
            {
                "position": arguments.start + offset,
                "expanded_minlex": case["expanded_minlex"],
                "published_rating": case["rating"],
                "actual_rating": rating,
                "matches": rating == case["rating"],
            }
        )
    return {
        "schema_version": 1,
        "generated_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "corpus": str(arguments.corpus.resolve()),
        "corpus_cases_sha256": document["cases_sha256"],
        "rater": str(rater),
        "rater_sha256": sha256(rater),
        "uniqueness_enabled": arguments.allow_uniqueness,
        "elapsed_seconds": elapsed,
        "matches": sum(result["matches"] for result in results),
        "results": results,
    }


def print_report(report: dict[str, Any]) -> None:
    print(f"{'row':>4}  {'published':>14}  {'actual':>14}  result")
    for result in report["results"]:
        status = "match" if result["matches"] else "DIFF"
        print(
            f"{result['position']:4d}  {result['published_rating']:>14}  "
            f"{result['actual_rating']:>14}  {status}"
        )
    total = len(report["results"])
    print(
        f"{report['matches']}/{total} match; "
        f"elapsed={report['elapsed_seconds']:.3f}s"
    )


def main(argv: Sequence[str] | None = None) -> int:
    arguments = parse_arguments(argv)
    if arguments.timeout <= 0:
        raise ValueError("--timeout must be positive")
    report = run_benchmark(arguments)
    print_report(report)
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
    except (CorpusBenchmarkError, FileNotFoundError, OSError, ValueError) as error:
        raise SystemExit(f"error: {error}") from error
