#!/usr/bin/env python3
"""Replay Java mid-trace states through the complete ported producer registry."""

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
CASE_IDS = ("classic_dynamic_forcing_chain", "anti_knight_forcing_chain")
PORTED_PREFIXES = (
    "Hidden Single:",
    "Naked Single:",
    "Direct Pointing:",
    "Direct Claiming:",
    "Direct Hidden Pair:",
    "Direct Hidden Triplet:",
    "Pointing:",
    "Claiming:",
    "Naked Pair:",
    "Generalized Naked Pair:",
    "X-Wing:",
    "Hidden Pair:",
    "Naked Triplet:",
    "Generalized Naked Triplet:",
    "Swordfish:",
    "Hidden Triplet:",
    "XY-Wing:",
    "XYZ-Wing:",
    "Naked Quad:",
    "Generalized Naked Quad:",
    "Jellyfish:",
    "Hidden Quad:",
    "Aligned Pair Exclusion:",
    "Aligned Triplet Exclusion:",
)
GENERALIZED_INTERSECTION_STEPS = {
    "anti_knight_forcing_chain": {
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        18,
        19,
        20,
        21,
        22,
        23,
        24,
        25,
        26,
        27,
        30,
        31,
        32,
        33,
        34,
        45,
        50,
        51,
        52,
        53,
        90,
        91,
    }
}
STRONG_LINK_STEPS = {
    "classic_dynamic_forcing_chain": {
        7: (
            "4.3",
            "Grouped 2 Strong links 101: Cell r2c4,r3c5,r3c9,r8c9 on value 3",
        ),
        14: (
            "4.1",
            "2-String Kite 012: Cell r2c7,r2c3,r1c2,r5c2 on value 6",
        ),
        21: (
            "4.0",
            "Skyscraper 011: Cell r9c5,r3c5,r3c9,r8c9 on value 3",
        ),
        22: (
            "4.3",
            "Grouped Skyscraper 111: Cell r3c5,r9c5,r9c8,r1c8 on value 6",
        ),
    },
    "anti_knight_forcing_chain": {
        54: (
            "4.3",
            "Grouped 2-String Kite 212: Cell r8c9,r8c1,r7c2,r5c2 on value 9",
        ),
    },
}
THREE_STRONG_LINK_STEPS = {
    "classic_dynamic_forcing_chain": {
        12: (
            "5.7",
            "Grouped 3 Strong links 1010: Cell r2c4,r3c5,r3c9,r8c9,r8c1,r7c3 on value 3",
        ),
    },
    "anti_knight_forcing_chain": {
        35: (
            "5.7",
            "Grouped 3 Strong links 2000: Cell r1c4,r2c5,r8c5,r9c4,r9c2,r7c1 on value 2",
        ),
        47: (
            "5.7",
            "Grouped 3 Strong links 3012: Cell r1c4,r3c5,r8c5,r8c1,r7c2,r1c2 on value 1",
        ),
        55: (
            "5.7",
            "Grouped 3 Strong links 2001: Cell r7c1,r9c2,r9c4,r8c5,r2c5,r2c1 on value 2",
        ),
    },
}
FOUR_STRONG_LINK_STEPS = {
    "anti_knight_forcing_chain": {
        56: (
            "6.1",
            "Grouped 4 Strong links 20121: Cell r1c3,r2c1,r2c7,r9c7,r8c9,r8c1,r7c2,r5c2 on value 9",
        ),
    },
}
WING_STEPS = {
    "classic_dynamic_forcing_chain": {
        13: (
            "4.2",
            "XY-Wing: Cells r2c4,r2c3,r3c5 on value 6",
        ),
    },
}
ALPHABET_WING_STEPS = {
    "classic_dynamic_forcing_chain": {
        15: (
            "5.5",
            "WXYZ-Wing 248: Cells r1c2,r2c3,r3c2,r4c2 on values 3,7",
        ),
    },
    "anti_knight_forcing_chain": {
        48: (
            "6.2",
            "VWXYZ-Wing 2513: Cells r1c3,r2c1,r2c2,r2c3,r2c5 on values 2,7",
        ),
        57: (
            "6.3",
            "VWXYZ-Wing 1412: Cells r7c1,r7c2,r8c1,r8c3,r5c2 on value 9",
        ),
        58: (
            "6.6",
            "UVWXYZ-Wing 1315: Cells r1c2,r2c2,r3c2,r7c2,r9c2,r1c3 on value 9",
        ),
    },
}
# All four alphabet wings now precede the next enabled unported registry slot.
# The committed traces contain no selected TUVWXYZ-Wing occurrence, so its
# positive-result coverage remains in direct Java-derived fixtures.
FULL_REGISTRY_ALPHABET_WING_STEPS = {
    "classic_dynamic_forcing_chain": {15},
    "anti_knight_forcing_chain": {48, 57, 58},
}
UNIQUE_LOOP_STEPS = {
    "classic_dynamic_forcing_chain": {
        23: (
            "4.5",
            "Unique Rectangle type 1: Cells r1c4,r1c5,r8c5,r8c4 on 5, 9",
        ),
    },
}
FORCING_CHAIN_CYCLE_STEPS = {
    "anti_knight_forcing_chain": {
        36: (
            "6.6",
            "Turbot Fish: r7c1.8 off",
        ),
    },
}
ALIGNED_TRIPLET_EXCLUSION_STEPS = {
    "anti_knight_forcing_chain": {
        38: (
            "7.5",
            "Aligned Triplet Exclusion: r2c3,r8c5,r1c3",
        ),
    },
}
NISHIO_FORCING_CHAIN_STEPS = {
    "anti_knight_forcing_chain": {
        37: (
            "7.7",
            "Nishio Forcing Chain: r1c3.3 on ==> r3c5.3 both on & off",
        ),
        39: (
            "7.7",
            "Nishio Forcing Chain: r7c9.9 on ==> r4c8.9 both on & off",
        ),
        40: (
            "7.8",
            "Nishio Forcing Chain: r5c8.7 on ==> r6c1.7 both on & off",
        ),
        41: (
            "8.2",
            "Nishio Forcing Chain: r9c1.2 on ==> r1c4.2 both on & off",
        ),
        46: (
            "7.7",
            "Nishio Forcing Chain: r5c2.1 on ==> r6c7.1 both on & off",
        ),
    },
}
MULTIPLE_FORCING_CHAIN_STEPS = {
    "classic_dynamic_forcing_chain": {
        8: (
            "8.3",
            "Cell Forcing Chains: r7c4 ==> r3c4.3 off",
        ),
        9: (
            "8.3",
            "Cell Forcing Chains: r7c4 ==> r9c4.3 off",
        ),
    },
    "anti_knight_forcing_chain": {
        42: (
            "8.3",
            "Cell Forcing Chains: r7c7 ==> r8c7.1 off",
        ),
        43: (
            "8.3",
            "Cell Forcing Chains: r7c7 ==> r8c9.1 off",
        ),
        44: (
            "8.3",
            "Cell Forcing Chains: r7c7 ==> r9c7.1 off",
        ),
    },
}
DYNAMIC_FORCING_CHAIN_STEPS = {
    "classic_dynamic_forcing_chain": {
        10: (
            "8.8",
            "Contradiction Forcing Chain: r3c2.3 on ==> "
            "r3c1.7 both on & off",
        ),
        11: (
            "8.9",
            "Cell Forcing Chains: r4c2 ==> r9c2.3 off",
        ),
    },
}
EXPECTED_REPLAY_COUNT = 184
EXPECTED_REVISED_STRONG_LINK_REPLAYS = 5
EXPECTED_REVISED_NISHIO_REPLAYS = 5
EXPECTED_REVISED_MULTIPLE_DYNAMIC_CHAIN_REPLAYS = 7


def parse_single_step(stdout: str) -> dict[str, str]:
    lines = [line for line in stdout.splitlines() if line]
    if len(lines) != 1 or not lines[0].startswith("STEP\t"):
        raise ValueError(f"expected one STEP line, got {stdout!r}")
    fields = lines[0].split("\t", 4)
    if len(fields) != 5:
        raise ValueError(f"malformed STEP line: {lines[0]!r}")
    return {
        "rating": fields[1],
        "description": fields[2],
        "grid": fields[3],
        "candidates": fields[4],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, default=DEFAULT_CASES)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    arguments = parser.parse_args()

    document = json.loads(arguments.cases.read_text(encoding="utf-8"))
    cases = {case["id"]: case for case in document["cases"]}
    for case_id, expected_steps in STRONG_LINK_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "StrongLinks(2) oracle identity changed"
                )
                return 1
    for case_id, expected_steps in THREE_STRONG_LINK_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "StrongLinks(3) oracle identity changed"
                )
                return 1
    for case_id, expected_steps in FOUR_STRONG_LINK_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "StrongLinks(4) oracle identity changed"
                )
                return 1
    for case_id, expected_steps in WING_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "XY-/XYZ-Wing oracle identity changed"
                )
                return 1
    for case_id, expected_steps in ALPHABET_WING_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "alphabet-wing oracle identity changed"
                )
                return 1
    for case_id, expected_steps in UNIQUE_LOOP_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "Unique Loop oracle identity changed"
                )
                return 1
    for case_id, expected_steps in FORCING_CHAIN_CYCLE_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "Forcing Chains & Cycles oracle identity changed"
                )
                return 1
    for case_id, expected_steps in ALIGNED_TRIPLET_EXCLUSION_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "Aligned Triplet Exclusion oracle identity changed"
                )
                return 1
    for case_id, expected_steps in NISHIO_FORCING_CHAIN_STEPS.items():
        steps = cases[case_id]["expected"]["steps"]
        for step_number, expected_identity in expected_steps.items():
            step = steps[step_number - 1]
            actual_identity = (step["rating"], step["description"])
            if actual_identity != expected_identity:
                print(
                    f"FAIL {case_id} step {step_number}: "
                    "Nishio Forcing Chain oracle identity changed"
                )
                return 1
    for family, expected_by_case in (
        ("Multiple Forcing Chain", MULTIPLE_FORCING_CHAIN_STEPS),
        ("Dynamic Forcing Chain", DYNAMIC_FORCING_CHAIN_STEPS),
    ):
        for case_id, expected_steps in expected_by_case.items():
            steps = cases[case_id]["expected"]["steps"]
            for step_number, expected_identity in expected_steps.items():
                step = steps[step_number - 1]
                actual_identity = (step["rating"], step["description"])
                if actual_identity != expected_identity:
                    print(
                        f"FAIL {case_id} step {step_number}: "
                        f"{family} oracle identity changed"
                    )
                    return 1
    checked = 0
    revised_strong_links_checked = 0
    revised_nishio_checked = 0
    revised_multiple_dynamic_chains_checked = 0
    failures = 0
    by_technique: dict[str, int] = {}
    for case_id in CASE_IDS:
        case = cases[case_id]
        steps = case["expected"]["steps"]
        for index in range(1, len(steps)):
            expected = steps[index]
            step_number = index + 1
            is_generalized_intersection = step_number in GENERALIZED_INTERSECTION_STEPS.get(
                case_id, set()
            )
            is_strong_link = step_number in STRONG_LINK_STEPS.get(case_id, {})
            is_three_strong_link = step_number in THREE_STRONG_LINK_STEPS.get(
                case_id, {}
            )
            is_four_strong_link = step_number in FOUR_STRONG_LINK_STEPS.get(
                case_id, {}
            )
            is_wing = step_number in WING_STEPS.get(case_id, {})
            is_alphabet_wing = step_number in FULL_REGISTRY_ALPHABET_WING_STEPS.get(
                case_id, set()
            )
            is_unique_loop = step_number in UNIQUE_LOOP_STEPS.get(case_id, {})
            is_forcing_chain_cycle = step_number in FORCING_CHAIN_CYCLE_STEPS.get(
                case_id, {}
            )
            is_aligned_triplet_exclusion = (
                step_number in ALIGNED_TRIPLET_EXCLUSION_STEPS.get(case_id, {})
            )
            is_nishio_forcing_chain = step_number in NISHIO_FORCING_CHAIN_STEPS.get(
                case_id, {}
            )
            is_multiple_forcing_chain = (
                step_number in MULTIPLE_FORCING_CHAIN_STEPS.get(case_id, {})
            )
            is_dynamic_forcing_chain = (
                step_number in DYNAMIC_FORCING_CHAIN_STEPS.get(case_id, {})
            )
            if (
                not is_generalized_intersection
                and not is_strong_link
                and not is_three_strong_link
                and not is_four_strong_link
                and not is_wing
                and not is_alphabet_wing
                and not is_unique_loop
                and not is_forcing_chain_cycle
                and not is_aligned_triplet_exclusion
                and not is_nishio_forcing_chain
                and not is_multiple_forcing_chain
                and not is_dynamic_forcing_chain
                and not expected["description"].startswith(PORTED_PREFIXES)
            ):
                continue
            previous = steps[index - 1]
            completed = subprocess.run(
                [
                    str(arguments.binary),
                    "next",
                    *case.get("args", []),
                    previous["grid"],
                    previous["candidates"],
                ],
                text=True,
                capture_output=True,
                timeout=10,
                check=False,
            )
            checked += 1
            if is_generalized_intersection:
                technique = "Generalized Intersections"
            elif is_strong_link:
                technique = "StrongLinks(2)"
            elif is_three_strong_link:
                technique = "StrongLinks(3)"
            elif is_four_strong_link:
                technique = "StrongLinks(4)"
            elif is_wing:
                technique = "XY-/XYZ-Wing"
            elif is_alphabet_wing:
                technique = expected["description"].split(" ", 1)[0]
            elif is_unique_loop:
                technique = "Unique Loop"
            elif is_forcing_chain_cycle:
                technique = "Forcing Chains & Cycles"
            elif is_aligned_triplet_exclusion:
                technique = "Aligned Triplet Exclusion"
            elif is_nishio_forcing_chain:
                technique = "Nishio Forcing Chains"
            elif is_multiple_forcing_chain:
                technique = "Multiple Forcing Chains"
            elif is_dynamic_forcing_chain:
                technique = "Dynamic Forcing Chains"
            else:
                technique = expected["description"].split(":", 1)[0]
            by_technique[technique] = by_technique.get(technique, 0) + 1
            label = f"{case_id} step {step_number}"
            if completed.returncode != 0:
                print(f"FAIL {label}: {completed.stderr.strip()}")
                failures += 1
                continue
            try:
                actual = parse_single_step(completed.stdout)
            except ValueError as error:
                print(f"FAIL {label}: {error}")
                failures += 1
                continue
            if actual == expected and (
                is_three_strong_link
                or is_four_strong_link
                or is_nishio_forcing_chain
                or is_multiple_forcing_chain
                or is_dynamic_forcing_chain
            ):
                revised = subprocess.run(
                    [
                        str(arguments.binary),
                        "next",
                        "--revised-rating=1",
                        *case.get("args", []),
                        previous["grid"],
                        previous["candidates"],
                    ],
                    text=True,
                    capture_output=True,
                    timeout=10,
                    check=False,
                )
                if is_nishio_forcing_chain:
                    revised_nishio_checked += 1
                elif is_multiple_forcing_chain or is_dynamic_forcing_chain:
                    revised_multiple_dynamic_chains_checked += 1
                else:
                    revised_strong_links_checked += 1
                try:
                    revised_actual = (
                        parse_single_step(revised.stdout)
                        if revised.returncode == 0
                        else None
                    )
                except ValueError:
                    revised_actual = None
                if revised_actual != expected:
                    print(
                        f"FAIL {label} revised: "
                        f"{revised.stderr.strip() or 'Java trace mismatch'}"
                    )
                    failures += 1
                continue
            if actual == expected:
                continue
            print(f"FAIL {label}: Java trace mismatch")
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

    if checked != EXPECTED_REPLAY_COUNT:
        print(
            f"FAIL replay selection: expected {EXPECTED_REPLAY_COUNT} snapshots, "
            f"selected {checked}"
        )
        failures += 1
    if revised_strong_links_checked != EXPECTED_REVISED_STRONG_LINK_REPLAYS:
        print(
            "FAIL revised StrongLinks(3/4) selection: expected "
            f"{EXPECTED_REVISED_STRONG_LINK_REPLAYS}, "
            f"selected {revised_strong_links_checked}"
        )
        failures += 1
    if revised_nishio_checked != EXPECTED_REVISED_NISHIO_REPLAYS:
        print(
            "FAIL revised Nishio selection: expected "
            f"{EXPECTED_REVISED_NISHIO_REPLAYS}, "
            f"selected {revised_nishio_checked}"
        )
        failures += 1
    if (
        revised_multiple_dynamic_chains_checked
        != EXPECTED_REVISED_MULTIPLE_DYNAMIC_CHAIN_REPLAYS
    ):
        print(
            "FAIL revised Multiple/Dynamic Forcing Chain selection: expected "
            f"{EXPECTED_REVISED_MULTIPLE_DYNAMIC_CHAIN_REPLAYS}, "
            f"selected {revised_multiple_dynamic_chains_checked}"
        )
        failures += 1
    if failures:
        print(f"{failures} of {checked} inference snapshot replay(s) failed")
        return 1
    breakdown = ", ".join(f"{name}={count}" for name, count in sorted(by_technique.items()))
    print(
        f"Verified {checked} Java inference snapshot replay(s) plus "
        f"{revised_strong_links_checked} revised StrongLinks(3/4) and "
        f"{revised_nishio_checked} revised Nishio and "
        f"{revised_multiple_dynamic_chains_checked} revised Multiple/Dynamic "
        f"Forcing Chain replay(s): {breakdown}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
