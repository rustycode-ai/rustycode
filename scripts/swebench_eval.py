#!/usr/bin/env python3
"""Evaluate existing SWE-bench patches by running FAIL_TO_PASS tests."""
import json
import subprocess
import sys
from pathlib import Path


def run_tests(repo_dir: Path, test_names: list[str]) -> tuple[bool, str]:
    """Run specific test names and return (passed, output)."""
    has_pytest = (repo_dir / "pytest.ini").exists() or (repo_dir / "pyproject.toml").exists()
    has_django = (repo_dir / "tests" / "runtests.py").exists()

    results = []
    all_passed = True
    for test in test_names:
        if has_django:
            module = test.split("(")[0].rsplit(".", 1)[0] if "(" in test else test
            cmd = f"python3 tests/runtests.py {module} --verbosity=2"
        else:
            cmd = f"python3 -m pytest {test} -x --tb=short --no-header -q 2>&1"

        result = subprocess.run(
            cmd, shell=True, cwd=repo_dir, capture_output=True, text=True, timeout=120,
        )
        passed = result.returncode == 0
        all_passed = all_passed and passed
        results.append(f"{'PASS' if passed else 'FAIL'}: {test}")
        if not passed:
            results.append(result.stdout[-1000:] if result.stdout else result.stderr[-1000:])

    return all_passed, "\n".join(results)


def main():
    instances_file = sys.argv[1] if len(sys.argv) > 1 else "/tmp/swe-bench-hard-10.json"
    work_dir = Path(sys.argv[2] if len(sys.argv) > 2 else "/tmp/swebench-experiment")
    apply_test_patch = "--apply-test-patch" in sys.argv

    with open(instances_file) as f:
        instances = json.load(f)

    resolved = 0
    patched = 0
    for inst in instances:
        iid = inst["instance_id"]
        ftp_tests = inst.get("FAIL_TO_PASS", inst.get("fail_to_pass", []))
        if isinstance(ftp_tests, str):
            ftp_tests = json.loads(ftp_tests)
        repo_dir = work_dir / iid

        if not repo_dir.exists():
            print(f"[SKIP] {iid} — repo not found")
            continue

        # Apply test_patch if requested (adds/modifies test files)
        if apply_test_patch:
            test_patch = inst.get("test_patch", "")
            if test_patch and test_patch.strip():
                subprocess.run(
                    ["git", "checkout", "--quiet", "."],
                    cwd=repo_dir, capture_output=True, text=True,
                )
                # Re-apply agent's model_patch first
                model_patch_file = None
                for pred_file in ["predictions_hard10_baseline.json", "predictions_hard10_enhanced.json"]:
                    p = Path(f"/Users/nat/dev/rustycode/{pred_file}")
                    if p.exists():
                        with open(p) as f:
                            for pred in json.load(f):
                                if pred["instance_id"] == iid and pred.get("model_patch"):
                                    model_patch_file = p
                                    break
                r = subprocess.run(
                    ["git", "apply", "--allow-empty"],
                    input=test_patch, cwd=repo_dir,
                    capture_output=True, text=True,
                )
                if r.returncode != 0:
                    print(f"[WARN ] {iid} — test_patch failed: {r.stderr[:100]}")

        # Check if there are uncommitted changes (patch applied)
        r = subprocess.run(
            ["git", "diff", "--quiet"],
            cwd=repo_dir, capture_output=True, text=True,
        )
        has_patch = r.returncode != 0

        if not has_patch:
            print(f"[EMPTY] {iid} — no patch applied")
            continue

        patched += 1

        if not ftp_tests:
            print(f"[?????] {iid} — no FAIL_TO_PASS tests listed")
            continue

        print(f"[TEST ] {iid} — running {len(ftp_tests)} test(s)...")
        passed, output = run_tests(repo_dir, ftp_tests)
        if passed:
            resolved += 1
            print(f"  ✅ RESOLVED")
        else:
            print(f"  ❌ FAILED")
            # Show first failure
            for line in output.split("\n"):
                if line.startswith("FAIL:"):
                    print(f"     {line}")
                    break

    total = len(instances)
    print(f"\n{'='*60}")
    print(f"  TOTAL: {total}  PATCHED: {patched}  RESOLVED: {resolved}")
    print(f"  Patch rate: {100*patched/total:.0f}%  Resolve rate: {100*resolved/total:.0f}%")
    print(f"{'='*60}")


if __name__ == "__main__":
    main()
