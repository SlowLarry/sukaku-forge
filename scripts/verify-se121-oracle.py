#!/usr/bin/env python3
"""Differentially verify the Classic rater against the pinned SE 1.2.1 oracle."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
from typing import Any
import zipfile


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_METADATA = Path(__file__).with_name("se121-oracle.json")
DEFAULT_CORPUS = Path(__file__).with_name("se121-classic-corpus.json")
DEFAULT_ORACLE_JAR = ROOT / "target" / "se121-oracle" / "serate.jar"
RATING_PATTERN = re.compile(r"^[0-9]+\.[0-9]/[0-9]+\.[0-9]/[0-9]+\.[0-9]$")


class VerificationError(RuntimeError):
    """A reproducibility or differential verification check failed."""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--classic-rater",
        required=True,
        type=Path,
        help="path to the sukaku-forge-rate executable",
    )
    parser.add_argument(
        "--oracle-jar", type=Path, default=DEFAULT_ORACLE_JAR
    )
    parser.add_argument("--metadata", type=Path, default=DEFAULT_METADATA)
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--java", default="java", help="Java executable to run")
    parser.add_argument("--timeout", type=float, default=30.0)
    return parser.parse_args()


def load_document(path: Path) -> dict[str, Any]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise VerificationError(f"cannot load {path}: {error}") from error
    if not isinstance(document, dict):
        raise VerificationError(f"{path} must contain a JSON object")
    return document


def verify_oracle_artifact(jar: Path, metadata: dict[str, Any]) -> None:
    try:
        data = jar.read_bytes()
    except OSError as error:
        raise VerificationError(f"cannot read oracle JAR {jar}: {error}") from error

    actual_size = len(data)
    expected_size = metadata.get("jar_size")
    if actual_size != expected_size:
        raise VerificationError(
            f"oracle JAR size is {actual_size}, expected {expected_size}"
        )
    actual_sha256 = hashlib.sha256(data).hexdigest()
    expected_sha256 = metadata.get("jar_sha256")
    if actual_sha256 != expected_sha256:
        raise VerificationError(
            f"oracle JAR SHA-256 is {actual_sha256}, expected {expected_sha256}"
        )

    try:
        with zipfile.ZipFile(jar) as archive:
            class_count = sum(
                entry.filename.endswith(".class") for entry in archive.infolist()
            )
            main_class = str(metadata.get("main_class", "")).replace(".", "/")
            main_entry = f"{main_class}.class"
            entries = {entry.filename for entry in archive.infolist()}
    except (OSError, zipfile.BadZipFile) as error:
        raise VerificationError(f"cannot inspect oracle JAR {jar}: {error}") from error

    expected_count = metadata.get("class_count")
    if class_count != expected_count:
        raise VerificationError(
            f"oracle JAR has {class_count} classes, expected {expected_count}"
        )
    if main_entry not in entries:
        raise VerificationError(f"oracle JAR is missing main class {main_entry}")


def verify_java_runtime(java: str, metadata: dict[str, Any]) -> None:
    try:
        result = subprocess.run(
            [java, "-version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=10.0,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise VerificationError(f"cannot identify Java runtime: {error}") from error
    if result.returncode != 0:
        raise VerificationError(
            f"java -version exited with status {result.returncode}"
        )
    actual_sha256 = hashlib.sha256(result.stdout).hexdigest()
    expected_sha256 = metadata.get("java_version_output_sha256")
    if actual_sha256 != expected_sha256:
        raise VerificationError(
            "Java runtime fingerprint is "
            f"{actual_sha256}, expected {expected_sha256}"
        )


def validate_corpus(document: dict[str, Any]) -> list[dict[str, str]]:
    if "schema_version" not in document:
        raise VerificationError("corpus is missing schema_version; expected 2")
    schema_version = document["schema_version"]
    if type(schema_version) is not int or schema_version != 2:
        raise VerificationError(
            f"unsupported corpus schema_version {schema_version!r}; expected 2"
        )
    if document.get("protected_cases_included") is not False:
        raise VerificationError("corpus must explicitly exclude protected cases")
    if document.get("format") != "%r/%p/%d":
        raise VerificationError("corpus format must be %r/%p/%d")
    cases = document.get("cases")
    if not isinstance(cases, list) or not cases:
        raise VerificationError("corpus must contain a non-empty cases array")

    validated: list[dict[str, str]] = []
    identifiers: set[str] = set()
    for index, value in enumerate(cases, start=1):
        if not isinstance(value, dict):
            raise VerificationError(f"corpus case {index} must be an object")
        identifier = value.get("id")
        puzzle = value.get("puzzle")
        rating = value.get("expected_rating")
        oracle_rating = value.get("expected_oracle_rating", rating)
        if not isinstance(identifier, str) or not identifier:
            raise VerificationError(f"corpus case {index} has no valid id")
        if identifier in identifiers:
            raise VerificationError(f"duplicate corpus case id {identifier}")
        identifiers.add(identifier)
        if (
            not isinstance(puzzle, str)
            or len(puzzle) != 81
            or any(character not in ".0123456789" for character in puzzle)
        ):
            raise VerificationError(
                f"corpus case {identifier} is not a Classic 81-cell puzzle"
            )
        if not isinstance(rating, str) or RATING_PATTERN.fullmatch(rating) is None:
            raise VerificationError(
                f"corpus case {identifier} has invalid expected rating"
            )
        if (
            not isinstance(oracle_rating, str)
            or RATING_PATTERN.fullmatch(oracle_rating) is None
        ):
            raise VerificationError(
                f"corpus case {identifier} has invalid expected oracle rating"
            )
        validated.append(
            {
                "id": identifier,
                "puzzle": puzzle,
                "expected_rating": rating,
                "expected_oracle_rating": oracle_rating,
            }
        )
    return validated


def run_rater(
    command: list[str], payload: str, expected_lines: int, timeout: float, label: str
) -> list[str]:
    try:
        result = subprocess.run(
            command,
            input=payload,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise VerificationError(f"failed to run {label}: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostics"
        raise VerificationError(
            f"{label} exited with status {result.returncode}: {detail}"
        )

    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if len(lines) != expected_lines:
        raise VerificationError(
            f"{label} returned {len(lines)} ratings, expected {expected_lines}: "
            f"{lines!r}"
        )
    for line in lines:
        if RATING_PATTERN.fullmatch(line) is None:
            raise VerificationError(f"{label} returned malformed rating {line!r}")
    return lines


def main() -> int:
    arguments = parse_args()
    metadata = load_document(arguments.metadata)
    corpus = validate_corpus(load_document(arguments.corpus))
    verify_oracle_artifact(arguments.oracle_jar, metadata)
    verify_java_runtime(arguments.java, metadata)

    payload = "".join(f"{case['puzzle']}\n" for case in corpus)
    main_class = metadata.get("main_class")
    if not isinstance(main_class, str) or not main_class:
        raise VerificationError("oracle metadata has no valid main_class")

    oracle_ratings: list[str] = []
    with tempfile.TemporaryDirectory(prefix="se121-oracle-prefs-") as preferences:
        for index, case in enumerate(corpus):
            oracle_command = [
                arguments.java,
                "-Xrs",
                "-Xmx500m",
                f"-Djava.util.prefs.userRoot={Path(preferences) / str(index)}",
                "-cp",
                str(arguments.oracle_jar),
                main_class,
                "--format=%r/%p/%d",
                "--input=-",
            ]
            oracle_ratings.extend(
                run_rater(
                    oracle_command,
                    f"{case['puzzle']}\n",
                    1,
                    arguments.timeout,
                    f"SE 1.2.1 oracle ({case['id']})",
                )
            )

    expected_oracle_ratings = [case["expected_oracle_rating"] for case in corpus]
    if oracle_ratings != expected_oracle_ratings:
        differences = [
            f"{case['id']}: oracle {actual}, frozen {expected}"
            for case, actual, expected in zip(
                corpus, oracle_ratings, expected_oracle_ratings
            )
            if actual != expected
        ]
        raise VerificationError(
            "oracle differs from frozen corpus: " + "; ".join(differences)
        )

    classic_rater = arguments.classic_rater
    if classic_rater.exists():
        classic_rater = classic_rater.resolve()
    classic_command = [
        str(classic_rater),
        "--format=%r/%p/%d",
        "--input=-",
        # The pinned Java oracle always schedules Unique Loops and BUG. The
        # product defaults them off, so parity verification opts in explicitly.
        "--allow-uniqueness",
    ]
    classic_ratings = run_rater(
        classic_command, payload, len(corpus), arguments.timeout, "Classic rater"
    )
    expected_ratings = [case["expected_rating"] for case in corpus]
    if classic_ratings != expected_ratings:
        differences = [
            f"{case['id']}: corrected Classic {classic}, frozen {expected}"
            for case, classic, expected in zip(corpus, classic_ratings, expected_ratings)
            if classic != expected
        ]
        raise VerificationError("rating mismatch: " + "; ".join(differences))

    intentional_differences = sum(
        expected != oracle
        for expected, oracle in zip(expected_ratings, expected_oracle_ratings)
    )

    print(
        f"verified {len(corpus)} corrected Classic ratings and their "
        f"SE 1.2.1 baselines ({metadata['jar_sha256']}); "
        f"documented bug-fix deltas: {intentional_differences}"
    )
    for case, rating in zip(corpus, classic_ratings):
        print(f"{case['id']}: {rating}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        raise SystemExit(f"error: {error}") from error
