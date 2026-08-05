#!/usr/bin/env python3
"""Compare complete Rust solve traces and result identities with Java."""

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
TRACE_EXPECTATIONS = {
    "classic_dynamic_forcing_chain": {
        "step_count": 72,
        "result": {
            "er": "8.9",
            "ep": "1.5",
            "ed": "1.5",
            "er_technique": "Dynamic Cell Forcing Chains",
            "ep_technique": "Hidden Single",
            "ed_technique": "Hidden Single",
        },
    },
    "anti_knight_forcing_chain": {
        "step_count": 114,
        "result": {
            "er": "8.3",
            "ep": "1.2",
            "ed": "1.2",
            "er_technique": "Cell Forcing Chains",
            "ep_technique": "Hidden Single",
            "ed_technique": "Hidden Single",
        },
    },
}


def parse_output(stdout: str) -> dict[str, object]:
    steps: list[dict[str, str]] = []
    result: dict[str, str] | None = None
    for line in stdout.splitlines():
        if line.startswith("STEP\t"):
            if result is not None:
                raise ValueError("step emitted after canonical RESULT")
            fields = line.split("\t", 4)
            if len(fields) != 5:
                raise ValueError(f"malformed STEP line: {line!r}")
            steps.append(
                {
                    "rating": fields[1],
                    "description": fields[2],
                    "grid": fields[3],
                    "candidates": fields[4],
                }
            )
        elif line.startswith("RESULT\t"):
            if result is not None:
                raise ValueError("trace emitted more than one canonical RESULT")
            fields = line.split("\t")
            if len(fields) != 7:
                raise ValueError(f"malformed RESULT line: {line!r}")
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
        raise ValueError("complete trace emitted no canonical RESULT")
    return {"result": result, "steps": steps}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, default=DEFAULT_CASES)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    arguments = parser.parse_args()

    document = json.loads(arguments.cases.read_text(encoding="utf-8"))
    cases = {case["id"]: case for case in document["cases"]}
    failures = 0
    for case_id, frozen in TRACE_EXPECTATIONS.items():
        case = cases[case_id]
        expected = case["expected"]
        if len(expected["steps"]) != frozen["step_count"]:
            print(
                f"FAIL {case_id}: committed Java step count changed from "
                f"{frozen['step_count']} to {len(expected['steps'])}"
            )
            failures += 1
            continue
        if expected["result"] != frozen["result"]:
            print(f"FAIL {case_id}: committed Java RESULT identity changed")
            failures += 1
            continue

        completed = subprocess.run(
            [str(arguments.binary), "trace", *case.get("args", []), case["puzzle"]],
            text=True,
            capture_output=True,
            timeout=case.get("timeout_seconds", 30),
            check=False,
        )
        label = f"{case_id} ({frozen['step_count']} steps)"
        if completed.returncode != 0:
            print(f"FAIL {label}: {completed.stderr.strip()}")
            failures += 1
            continue
        try:
            actual = parse_output(completed.stdout)
        except ValueError as error:
            print(f"FAIL {label}: {error}")
            failures += 1
            continue
        if actual == expected:
            print(
                f"PASS {label}, RESULT "
                f"{actual['result']['er']}/{actual['result']['ep']}/"
                f"{actual['result']['ed']}"
            )
            continue
        print(f"FAIL {label}: Java full-trace mismatch")
        print(
            "\n".join(
                difflib.unified_diff(
                    json.dumps(expected, indent=2, sort_keys=True).splitlines(),
                    json.dumps(actual, indent=2, sort_keys=True).splitlines(),
                    fromfile="java",
                    tofile="rust",
                    lineterm="",
                )
            )
        )
        failures += 1

    if failures:
        return 1
    print(f"Verified {len(TRACE_EXPECTATIONS)} complete Java solve traces and RESULTs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
