#!/usr/bin/env python3
"""SWE-bench evaluation wrapper using the official swebench package.

Usage:
    python3 swebench_evaluate.py \
        --predictions /tmp/swe-predictions.json \
        --instances /tmp/swe-bench_lite.json \
        --instance-ids django__django-10914,matplotlib__matplotlib-23314 \
        --output /tmp/swe-eval-results.json

Requires: swebench (pip install swebench), Docker
"""

import argparse
import json
import sys
import os


def main():
    parser = argparse.ArgumentParser(description="SWE-bench evaluation with Docker")
    parser.add_argument("--predictions", required=True, help="Path to predictions JSON")
    parser.add_argument("--instances", required=True, help="Path to SWE-bench instances JSON")
    parser.add_argument("--instance-ids", default=None, help="Comma-separated instance IDs")
    parser.add_argument("--output", default=None, help="Output path for results JSON")
    parser.add_argument("--max-workers", type=int, default=1, help="Max parallel evaluations")
    parser.add_argument("--run-id", default="rtk-eval", help="Run ID for Docker containers")
    parser.add_argument("--timeout", type=int, default=900, help="Per-instance timeout in seconds")
    args = parser.parse_args()

    try:
        from swebench.harness.run_evaluation import main as swebench_main
    except ImportError:
        print("ERROR: swebench not installed. Run: pip install swebench", file=sys.stderr)
        sys.exit(1)

    # Load predictions to get model name and count
    with open(args.predictions) as f:
        predictions = json.load(f)

    model_name = "unknown"
    if predictions and "model_name_or_path" in predictions[0]:
        model_name = predictions[0]["model_name_or_path"]

    # Filter instance IDs if specified
    instance_ids = []
    if args.instance_ids:
        instance_ids = [s.strip() for s in args.instance_ids.split(",") if s.strip()]
        # Filter predictions file to only include requested instances
        predictions = [p for p in predictions if p["instance_id"] in instance_ids]
        # Write filtered predictions to temp file
        filtered_path = args.predictions + ".filtered.json"
        with open(filtered_path, "w") as f:
            json.dump(predictions, f, indent=2)
        predictions_path = filtered_path
    else:
        predictions_path = args.predictions

    print(f"Evaluating {len(predictions)} predictions with swebench (Docker)...")
    print(f"  Predictions: {args.predictions}")
    print(f"  Dataset: {args.instances}")
    print(f"  Model: {model_name}")
    print(f"  Max workers: {args.max_workers}")

    try:
        swebench_main(
            dataset_name=args.instances,
            split="test",
            instance_ids=instance_ids if instance_ids else [],
            predictions_path=predictions_path,
            max_workers=args.max_workers,
            run_id=args.run_id,
            timeout=args.timeout,
            cache_level="env",
            clean=False,
            force_rebuild=False,
            open_file_limit=4096,
            modal=False,
            namespace="swebench",
            rewrite_reports=False,
        )
        print("\nEvaluation complete.")
    except Exception as e:
        print(f"\nEvaluation error: {e}", file=sys.stderr)
        sys.exit(1)

    # Parse results from swebench report
    # Report file pattern: {model_name}.{run_id}.json
    import glob as globmod
    report_candidates = sorted(globmod.glob(f"*.{args.run_id}.json"), key=os.path.getmtime, reverse=True)
    results = []

    if report_candidates:
        with open(report_candidates[0]) as f:
            report = json.load(f)
        resolved_ids = set(report.get("resolved_ids", []))
        error_ids = set(report.get("error_ids", []))
        for pred in predictions:
            inst_id = pred["instance_id"]
            results.append({
                "instance_id": inst_id,
                "resolved": inst_id in resolved_ids,
                "error": "eval_error" if inst_id in error_ids else None,
            })
    else:
        # Fallback: all unresolved
        for pred in predictions:
            results.append({
                "instance_id": pred["instance_id"],
                "resolved": False,
                "error": "report not found",
            })

    if args.output and results:
        with open(args.output, "w") as f:
            json.dump(results, f, indent=2)
        print(f"Results saved to {args.output}")

    # Print summary
    resolved_count = sum(1 for r in results if r.get("resolved"))
    print(f"\n{'='*40}")
    print(f"Resolved: {resolved_count}/{len(results)}")
    print(f"{'='*40}")


if __name__ == "__main__":
    main()
