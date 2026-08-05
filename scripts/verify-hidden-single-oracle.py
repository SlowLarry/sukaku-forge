#!/usr/bin/env python3
"""Compare the first Rust vertical slice with committed Java trace snapshots."""

from __future__ import annotations

import argparse
import difflib
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CASES = ROOT.parent / "sukaku-explainer" / "oracle" / "cases.json"
DEFAULT_BINARY = ROOT / "target" / "debug" / "sukaku-forge"
CASE_IDS = (
    "classic_hidden_single",
    "classic_visibility_control",
    "anti_knight_visibility_order",
    "non_consecutive_candidate_pruning",
    "non_consecutive_plus_candidate_pruning",
)


def parse_output(stdout: str) -> dict[str, object]:
    steps: list[dict[str, str]] = []
    result: dict[str, str] | None = None
    for line in stdout.splitlines():
        if line.startswith("STEP\t"):
            fields = line.split("\t", 4)
            if len(fields) != 5:
                raise ValueError(f"malformed step line: {line!r}")
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
                raise ValueError(f"malformed result line: {line!r}")
            result = {
                "er": fields[1],
                "ep": fields[2],
                "ed": fields[3],
                "er_technique": fields[4],
                "ep_technique": fields[5],
                "ed_technique": fields[6],
            }
        elif line.strip():
            raise ValueError(f"unexpected output line: {line!r}")
    if result is None:
        raise ValueError("trace has no result")
    return {"result": result, "steps": steps}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, default=DEFAULT_CASES)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    arguments = parser.parse_args()

    document = json.loads(arguments.cases.read_text(encoding="utf-8"))
    cases = {case["id"]: case for case in document["cases"]}
    failures = 0
    for case_id in CASE_IDS:
        case = cases[case_id]
        completed = subprocess.run(
            [str(arguments.binary), "trace", *case.get("args", []), case["puzzle"]],
            text=True,
            capture_output=True,
            timeout=10,
            check=False,
        )
        if completed.returncode != 0:
            print(f"FAIL {case_id}: {completed.stderr.strip()}")
            failures += 1
            continue
        actual = parse_output(completed.stdout)
        expected = case["expected"]
        if actual == expected:
            print(f"PASS {case_id} ({len(actual['steps'])} steps)")
            continue
        print(f"FAIL {case_id}: Java trace mismatch")
        expected_lines = json.dumps(expected, indent=2, sort_keys=True).splitlines()
        actual_lines = json.dumps(actual, indent=2, sort_keys=True).splitlines()
        print(
            "\n".join(
                difflib.unified_diff(
                    expected_lines,
                    actual_lines,
                    fromfile="java",
                    tofile="rust",
                    lineterm="",
                )
            )
        )
        failures += 1
    if failures:
        print(f"{failures} of {len(CASE_IDS)} Hidden Single case(s) failed")
        return 1
    print(f"Verified {len(CASE_IDS)} Java Hidden Single trace case(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
