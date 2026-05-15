#!/usr/bin/env python3
"""Compare generated patches against gold patches to estimate correctness."""
import json
import re
import sys
from pathlib import Path


def extract_file_changes(patch: str) -> dict[str, list[str]]:
    """Extract changed files and their hunks from a diff."""
    if not patch or not patch.strip():
        return {}

    changes = {}
    current_file = None
    current_hunks = []

    for line in patch.split("\n"):
        if line.startswith("diff --git"):
            if current_file and current_hunks:
                changes[current_file] = current_hunks
            # Extract file path from "diff --git a/path b/path"
            match = re.match(r"diff --git a/(.+?) b/(.+)", line)
            current_file = match.group(2) if match else None
            current_hunks = []
        elif line.startswith("@@"):
            current_hunks.append(line)
        elif current_file and (line.startswith("+") or line.startswith("-")) and not line.startswith("+++") and not line.startswith("---"):
            # Strip the +/- prefix for comparison
            current_hunks.append(line[1:].strip())

    if current_file and current_hunks:
        changes[current_file] = current_hunks

    return changes


def compare_patches(gold: str, generated: str) -> dict:
    """Compare a generated patch against the gold patch."""
    if not generated or not generated.strip():
        return {"status": "EMPTY", "score": 0.0, "details": "No patch generated"}

    gold_files = extract_file_changes(gold)
    gen_files = extract_file_changes(generated)

    if not gold_files:
        return {"status": "NO_GOLD", "score": 0.0, "details": "No gold patch available"}

    # Check file overlap
    gold_file_set = set(gold_files.keys())
    gen_file_set = set(gen_files.keys())

    common = gold_file_set & gen_file_set
    only_gold = gold_file_set - gen_file_set
    only_gen = gen_file_set - gold_file_set

    if not common:
        return {
            "status": "WRONG_FILES",
            "score": 0.0,
            "details": f"Gold: {gold_file_set}, Gen: {gen_file_set}",
        }

    # Check hunk overlap for common files
    total_gold_lines = 0
    matched_lines = 0

    for f in common:
        gold_lines = set(l for l in gold_files[f] if l and not l.startswith("@@"))
        gen_lines = set(l for l in gen_files[f] if l and not l.startswith("@@"))

        total_gold_lines += len(gold_lines)
        matched_lines += len(gold_lines & gen_lines)

    for f in only_gold:
        gold_lines = set(l for l in gold_files[f] if l and not l.startswith("@@"))
        total_gold_lines += len(gold_lines)

    if total_gold_lines == 0:
        overlap = 1.0 if common else 0.0
    else:
        overlap = matched_lines / total_gold_lines

    # Classify
    if overlap >= 0.8:
        status = "LIKELY_CORRECT"
    elif overlap >= 0.4:
        status = "PARTIAL"
    elif overlap > 0:
        status = "POOR"
    else:
        status = "WRONG_APPROACH"

    return {
        "status": status,
        "score": overlap,
        "common_files": len(common),
        "gold_only_files": len(only_gold),
        "gen_only_files": len(only_gen),
        "gold_files": sorted(gold_file_set),
        "gen_files": sorted(gen_file_set),
        "line_overlap": f"{matched_lines}/{total_gold_lines}",
    }


def main():
    instances_file = sys.argv[1] if len(sys.argv) > 1 else "/tmp/swe-bench-hard-10.json"
    predictions_file = sys.argv[2] if len(sys.argv) > 2 else None

    if not predictions_file:
        print("Usage: python3 swebench_diffcompare.py <instances.json> <predictions.json>")
        sys.exit(1)

    with open(instances_file) as f:
        instances = {i["instance_id"]: i for i in json.load(f)}

    with open(predictions_file) as f:
        predictions = json.load(f)

    results = {"LIKELY_CORRECT": 0, "PARTIAL": 0, "POOR": 0, "WRONG_APPROACH": 0, "EMPTY": 0}

    for pred in predictions:
        iid = pred["instance_id"]
        inst = instances.get(iid, {})
        gold = inst.get("patch", "")
        generated = pred.get("model_patch", "")

        comp = compare_patches(gold, generated)
        results[comp["status"]] = results.get(comp["status"], 0) + 1

        score_bar = "█" * int(comp.get("score", 0) * 20) + "░" * (20 - int(comp.get("score", 0) * 20))
        print(f"[{comp['status']:15s}] {score_bar} {comp.get('score', 0):.0%} {iid}")
        if comp.get("details"):
            print(f"                {comp['details']}")
        if comp.get("gold_files") and comp.get("gen_files"):
            print(f"                Gold files: {comp['gold_files']}")
            print(f"                Gen files:  {comp['gen_files']}")
            if comp.get("line_overlap"):
                print(f"                Line overlap: {comp['line_overlap']}")
        print()

    total = len(predictions)
    likely = results.get("LIKELY_CORRECT", 0)
    partial = results.get("PARTIAL", 0)
    print(f"{'='*60}")
    print(f"  TOTAL: {total}")
    print(f"  LIKELY_CORRECT: {likely} ({100*likely/total:.0f}%)")
    print(f"  PARTIAL: {partial} ({100*partial/total:.0f}%)")
    print(f"  POOR: {results.get('POOR', 0)}")
    print(f"  WRONG_APPROACH: {results.get('WRONG_APPROACH', 0)}")
    print(f"  EMPTY: {results.get('EMPTY', 0)}")
    print(f"  Estimated resolve rate: {100*(likely+partial)//total}-{100*(likely+partial+1)//total}%")
    print(f"{'='*60}")


if __name__ == "__main__":
    main()
