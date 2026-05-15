#!/usr/bin/env python3
"""SWE-bench experiment runner — fast Python iteration for approach testing.

Usage:
    # Run single instance
    python3 scripts/swebench_experiment.py --instance mwaskom__seaborn-3187

    # Run multiple instances
    python3 scripts/swebench_experiment.py --instances /tmp/swebench-test-24.json --limit 5

    # Custom model
    python3 scripts/swebench_experiment.py --instance django__django-11951 --model claude-opus-4-7

    # Verbose (show tool calls)
    python3 scripts/swebench_experiment.py --instance sympy__sympy-12481 --verbose
"""

import anthropic
import json
import os
import re
import subprocess
import sys
import time
import argparse
import tempfile
import shutil
from pathlib import Path

# ── Tool Definitions ──────────────────────────────────────────────────

TOOLS = [
    {
        "name": "Bash",
        "description": "Run a bash command. Use for running tests, installing deps, git operations, grep, find. Always prefer running the actual failing test over writing scripts.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The bash command to run"},
                "timeout": {"type": "integer", "description": "Timeout in seconds", "default": 120},
            },
            "required": ["command"],
        },
    },
    {
        "name": "Read",
        "description": "Read a file's contents.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file (relative to repo root)"},
                "offset": {"type": "integer", "description": "Line offset (1-based)"},
                "limit": {"type": "integer", "description": "Max lines to read"},
            },
            "required": ["path"],
        },
    },
    {
        "name": "Write",
        "description": "Write content to a file. Creates or overwrites.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file (relative to repo root)"},
                "content": {"type": "string", "description": "File content to write"},
            },
            "required": ["path", "content"],
        },
    },
    {
        "name": "Edit",
        "description": "Replace exact text in a file. The old_text must match exactly (including whitespace/indentation). Use Read first to get exact content.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file (relative to repo root)"},
                "old_text": {"type": "string", "description": "Exact text to find"},
                "new_text": {"type": "string", "description": "Replacement text"},
            },
            "required": ["path", "old_text", "new_text"],
        },
    },
    {
        "name": "Grep",
        "description": "Search for a pattern in files. Use to find where functions/classes/variables are defined or used. Prefer targeted searches over broad ones.",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Regex pattern to search"},
                "path": {"type": "string", "description": "Directory to search (default: .)"},
                "include": {"type": "string", "description": "File glob to include (e.g. '*.py')"},
            },
            "required": ["pattern"],
        },
    },
    {
        "name": "Glob",
        "description": "Find files matching a pattern.",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Glob pattern (e.g. '**/*.py')"},
                "path": {"type": "string", "description": "Directory to search (default: .)"},
            },
            "required": ["pattern"],
        },
    },
    {
        "name": "ListDir",
        "description": "List directory contents.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Directory path (default: .)"},
            },
            "required": [],
        },
    },
    {
        "name": "FindReferences",
        "description": "Find ALL references to a symbol (function, class, variable) across source files. "
        "Automatically excludes test directories and __pycache__. Use this BEFORE editing to understand "
        "how many files need changes. Returns file paths with line numbers.",
        "input_schema": {
            "type": "object",
            "properties": {
                "symbol": {"type": "string", "description": "The symbol name to find references for"},
                "include_tests": {"type": "boolean", "description": "Include test files (default: false)", "default": False},
            },
            "required": ["symbol"],
        },
    },
    {
        "name": "GetSymbols",
        "description": "List all functions, classes, and top-level definitions in a file. "
        "Use to understand a file's structure before reading it.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file"},
            },
            "required": ["path"],
        },
    },
    {
        "name": "GitDiff",
        "description": "Show git diff of uncommitted changes. Use to review your edits before declaring done.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Specific file or directory to diff (default: all changes)"},
            },
            "required": [],
        },
    },
    {
        "name": "TodoWrite",
        "description": "Write or update a todo list to track multi-step fixes. Use when a fix requires 3+ steps.",
        "input_schema": {
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string", "description": "What needs to be done"},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"], "description": "Status of this item"},
                        },
                        "required": ["content", "status"],
                    },
                    "description": "List of todo items with content and status",
                },
            },
            "required": ["todos"],
        },
    },
]


# ── Tool Execution ────────────────────────────────────────────────────

def run_tool(name, input_args, repo_dir, verbose=False):
    """Execute a tool call and return the result string."""
    if name == "Bash":
        cmd = input_args["command"]
        timeout = input_args.get("timeout", 120)
        if verbose:
            print(f"    $ {cmd}")
        try:
            result = subprocess.run(
                cmd, shell=True, cwd=repo_dir,
                capture_output=True, text=True, timeout=timeout,
            )
            output = result.stdout
            if result.stderr and result.returncode != 0:
                output += f"\nSTDERR: {result.stderr}"
            if result.returncode != 0:
                output += f"\nEXIT CODE: {result.returncode}"
            return output[:50_000]  # Truncate large outputs
        except subprocess.TimeoutExpired:
            return f"ERROR: Command timed out after {timeout}s"

    elif name == "Read":
        path = input_args["path"]
        offset = input_args.get("offset")
        limit = input_args.get("limit")
        full_path = repo_dir / path
        if verbose:
            print(f"    READ {path}" + (f" L{offset}-{offset+limit}" if offset else ""))
        try:
            lines = full_path.read_text(errors="replace").splitlines(keepends=True)
            start = (offset or 1) - 1
            end = start + limit if limit else len(lines)
            return "".join(lines[start:end])[:50_000]
        except FileNotFoundError:
            return f"ERROR: File not found: {path}"
        except Exception as e:
            return f"ERROR: {e}"

    elif name == "Write":
        path = input_args["path"]
        if path.startswith("/"):
            return f"ERROR: Absolute paths not allowed: {path}. Use relative paths."
        content = input_args["content"]
        full_path = repo_dir / path
        if verbose:
            print(f"    WRITE {path} ({len(content)} bytes)")
        full_path.parent.mkdir(parents=True, exist_ok=True)
        full_path.write_text(content)
        return f"Wrote {len(content)} bytes to {path}"

    elif name == "Edit":
        path = input_args["path"]
        if path.startswith("/"):
            return f"ERROR: Absolute paths not allowed: {path}. Use relative paths."
        old_text = input_args["old_text"]
        new_text = input_args["new_text"]
        full_path = repo_dir / path
        if verbose:
            print(f"    EDIT {path}")
        try:
            content = full_path.read_text()
            if old_text not in content:
                # Try line-ending normalized match
                normalized = content.replace("\r\n", "\n")
                old_normalized = old_text.replace("\r\n", "\n")
                if old_normalized in normalized:
                    content = normalized
                    old_text = old_normalized
                else:
                    return f"ERROR: old_text not found in {path}"
            content = content.replace(old_text, new_text, 1)
            full_path.write_text(content)
            return f"Edited {path}"
        except FileNotFoundError:
            return f"ERROR: File not found: {path}"
        except Exception as e:
            return f"ERROR: {e}"

    elif name == "Grep":
        pattern = input_args["pattern"]
        path = input_args.get("path", ".")
        include = input_args.get("include")
        if verbose:
            print(f"    GREP '{pattern}' in {path}" + (f" ({include})" if include else ""))
        cmd = ["grep", "-rn", "-E", pattern, path]
        if include:
            cmd.extend(["--include", include])
        try:
            result = subprocess.run(cmd, cwd=repo_dir, capture_output=True, text=True, timeout=30)
            output = result.stdout or "(no matches)"
            return output[:30_000]
        except Exception as e:
            return f"ERROR: {e}"

    elif name == "Glob":
        pattern = input_args["pattern"]
        path = input_args.get("path", ".")
        if verbose:
            print(f"    GLOB '{pattern}' in {path}")
        matches = list((repo_dir / path).glob(pattern))[:200]
        return "\n".join(str(m.relative_to(repo_dir)) for m in matches) or "(no matches)"

    elif name == "ListDir":
        path = input_args.get("path", ".")
        if verbose:
            print(f"    LS {path}")
        full = repo_dir / path
        if not full.is_dir():
            return f"ERROR: {path} is not a directory"
        entries = sorted(full.iterdir(), key=lambda p: (not p.is_dir(), p.name))
        return "\n".join(
            f"{'[DIR] ' if e.is_dir() else '      '}{e.name}" for e in entries[:200]
        )

    elif name == "FindReferences":
        symbol = input_args["symbol"]
        include_tests = input_args.get("include_tests", False)
        if verbose:
            print(f"    FINDREFS '{symbol}' tests={include_tests}")
        cmd = ["grep", "-rn", "-E", rf"\b{re.escape(symbol)}\b", "."]
        cmd.extend(["--include", "*.py"])
        try:
            result = subprocess.run(cmd, cwd=repo_dir, capture_output=True, text=True, timeout=30)
            lines = result.stdout.splitlines()
            # Filter test dirs unless requested
            if not include_tests:
                lines = [l for l in lines
                         if "/test" not in l and "/tests/" not in l and "__pycache__" not in l]
            # Count unique files
            files = set()
            for l in lines:
                parts = l.split(":", 1)
                if parts:
                    files.add(parts[0])
            header = f"Found {len(lines)} references in {len(files)} files:\n"
            output = header + "\n".join(lines[:200])
            return output[:30_000]
        except Exception as e:
            return f"ERROR: {e}"

    elif name == "GetSymbols":
        path = input_args["path"]
        if verbose:
            print(f"    SYMBOLS {path}")
        full_path = repo_dir / path
        try:
            content = full_path.read_text(errors="replace")
            lines = content.splitlines()
            symbols = []
            for i, line in enumerate(lines, 1):
                stripped = line.strip()
                if stripped.startswith("def ") or stripped.startswith("class "):
                    symbols.append(f"  L{i}: {stripped.rstrip(':')}")
                elif stripped.startswith("async def "):
                    symbols.append(f"  L{i}: {stripped.rstrip(':')}")
            if not symbols:
                return "(no functions or classes found)"
            return f"{path} — {len(symbols)} symbols:\n" + "\n".join(symbols[:100])
        except FileNotFoundError:
            return f"ERROR: File not found: {path}"
        except Exception as e:
            return f"ERROR: {e}"

    elif name == "GitDiff":
        path = input_args.get("path", "")
        if verbose:
            print(f"    GITDIFF {path or 'all'}")
        cmd = f"git diff"
        if path:
            cmd += f" -- {path}"
        try:
            result = subprocess.run(
                cmd, shell=True, cwd=repo_dir,
                capture_output=True, text=True, timeout=30,
            )
            output = result.stdout
            if not output.strip():
                return "No uncommitted changes found."
            return output[:30_000]
        except subprocess.TimeoutExpired:
            return "ERROR: git diff timed out"

    elif name == "TodoWrite":
        todos = input_args.get("todos", [])
        if verbose:
            print(f"    TODOWRITE ({len(todos)} items)")
        lines = ["## Todo List"]
        for i, todo in enumerate(todos, 1):
            status = todo.get("status", "pending")
            content = todo.get("content", "")
            marker = {"completed": "[x]", "in_progress": "[>]", "pending": "[ ]"}.get(status, "[ ]")
            lines.append(f"{i}. {marker} {content}")
        return "\n".join(lines)

    return f"ERROR: Unknown tool {name}"


# ── SWE-bench Instance Setup ─────────────────────────────────────────

def load_instance(instance_id, instances_file="/tmp/swe-bench-verified.json"):
    with open(instances_file) as f:
        data = json.load(f)
    for inst in data:
        if inst["instance_id"] == instance_id:
            return inst
    raise ValueError(f"Instance {instance_id} not found")


def setup_repo(inst, work_dir="/tmp/swebench-experiment"):
    """Clone and checkout the instance's repo. Returns repo path."""
    inst_dir = Path(work_dir) / inst["instance_id"]
    clone_dir = inst_dir / "repo"

    if clone_dir.joinpath(".git").exists():
        # Reset to clean state
        subprocess.run(["git", "checkout", "--quiet", "."], cwd=clone_dir, capture_output=True)
        subprocess.run(["git", "clean", "-fdq"], cwd=clone_dir, capture_output=True)
    else:
        inst_dir.mkdir(parents=True, exist_ok=True)
        repo_url = f"https://github.com/{inst['repo']}.git"
        print(f"  Cloning {inst['repo']}...")
        subprocess.run(
            ["git", "clone", "--quiet", repo_url, str(clone_dir)],
            cwd=inst_dir, check=True, capture_output=True,
        )

    # Fetch and checkout base commit
    subprocess.run(
        ["git", "fetch", "--quiet", "origin", inst["base_commit"]],
        cwd=clone_dir, capture_output=True,
    )
    subprocess.run(
        ["git", "checkout", "--quiet", inst["base_commit"]],
        cwd=clone_dir, check=True, capture_output=True,
    )

    # Apply test_patch (adds/modifies test files needed for FAIL_TO_PASS)
    test_patch = inst.get("test_patch", "")
    if test_patch and test_patch.strip():
        r = subprocess.run(
            ["git", "apply", "--allow-empty"],
            input=test_patch, cwd=clone_dir,
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            print(f"  Warning: test_patch failed to apply: {r.stderr[:200]}")

    return clone_dir


def build_file_tree(repo_dir, max_depth=3):
    """Build a simple file tree for context."""
    result = []
    def walk(path, depth):
        if depth > max_depth:
            return
        try:
            entries = sorted(path.iterdir(), key=lambda p: (not p.is_dir(), p.name))
        except PermissionError:
            return
        for e in entries:
            if e.name.startswith(".") and e.name != ".github":
                continue
            rel = e.relative_to(repo_dir)
            if e.is_dir():
                result.append(f"{'  ' * depth}{e.name}/")
                walk(e, depth + 1)
            else:
                result.append(f"{'  ' * depth}{e.name}")
    walk(repo_dir, 0)
    return "\n".join(result[:200])  # Limit size


def capture_diff(repo_dir):
    result = subprocess.run(
        ["git", "diff"], cwd=repo_dir, capture_output=True, text=True,
    )
    return result.stdout


def run_tests(repo_dir, test_names):
    """Run specific test names and return (passed, output)."""
    if not test_names:
        return True, "(no tests)"

    # Detect test runner
    has_pytest = (repo_dir / "pytest.ini").exists() or (repo_dir / "pyproject.toml").exists()
    has_django = (repo_dir / "tests" / "runtests.py").exists()

    results = []
    all_passed = True
    for test in test_names:
        if has_django:
            # Extract module from Django test format
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
            results.append(result.stdout[-2000:] if result.stdout else result.stderr[-2000:])

    return all_passed, "\n".join(results)


# ── Agent Loop ────────────────────────────────────────────────────────

# ── System Prompt Variants ─────────────────────────────────────────────

SYSTEM_PROMPTS = {
    "minimal": "You are RustyCode, an AI coding assistant. Output complete working code.",
    "enhanced": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs, no explanations of what you would do.\n"
        "\n"
        "## Workflow\n"
        "\n"
        "For bug fixes and small changes:\n"
        "1. Read the error or failing test output (1 turn)\n"
        "2. Locate the relevant code with grep/read (1-2 turns, batch independent reads)\n"
        "3. Edit the file with the minimal fix (1 turn)\n"
        "4. Verify by running the relevant test (1 turn)\n"
        "Target: 4-6 turns. If you understand the bug after reading the error, skip step 2 and fix directly.\n"
        "\n"
        "For complex tasks (multi-file, refactors, new features):\n"
        "1. Scope: read the key files to understand the change surface (2-3 turns)\n"
        "2. Plan: write a numbered list of specific edits needed (1 turn)\n"
        "3. Execute: make edits one by one, verifying after each (N turns)\n"
        "4. Verify: run build/test/lint on the full change (1 turn)\n"
        "After each step: which step am I on? How many remain? Does the next step still make sense?\n"
        "\n"
        "## Self-check\n"
        "\n"
        "Before each turn, ask:\n"
        "- Am I closer to done than last turn? If not, stop exploring and act now.\n"
        "- Am I going backwards? (re-reading files, re-running searches already done) If yes, make your best edit instead.\n"
        "- Do I understand enough to edit right now? If yes, edit — don't read more for confirmation.\n"
        "- If the same approach fails twice → switch strategy entirely\n"
        "- If tests fail after editing → re-read the error output, don't guess at a fix\n"
        "- Before saying 'done' → verify: did I run the tests? did they pass?\n"
        "\n"
        "## Decision shortcuts\n"
        "\n"
        "- Confident in the fix? Edit now. Don't read more files for reassurance.\n"
        "- Past halfway through your turn budget with no edit? Stop reading, make your best fix.\n"
        "- Made an edit and tests pass? You're done. Don't look for more things to change.\n"
        "- Error message clearly shows the problem? Fix it directly — skip exploration.\n"
        "\n"
        "## Rules\n"
        "\n"
        "- Read files before modifying them\n"
        "- Make targeted changes, not broad refactors\n"
        "- Run tests to verify your changes\n"
        "- Use parallel tool calls when operations are independent\n"
        "- After making changes, always verify (build/test/lint) before declaring success\n"
        "- If repeating the same failed approach, switch strategy rather than retrying\n"
        "\n"
        "## Anti-patterns\n"
        "\n"
        "- Writing reproduction scripts when error output is already available\n"
        "- Reading files unrelated to the task out of curiosity\n"
        "- Re-reading a file you already have in context\n"
        "- Exploring for more than 3 turns without making an edit\n"
        "- Writing test scripts to verify when you can just run the existing tests\n"
        "- Continuing to edit after tests pass — ship what works\n"
        "\n"
        "## Before saying 'done'\n"
        "\n"
        "- Run the specific failing test — does it pass now?\n"
        "- Run the full test suite — no regressions?\n"
        "- Check for import errors or syntax issues\n"
        "- Does the fix address the root cause, not just the symptom?\n"
        "\n"
        "## When stuck\n"
        "\n"
        "- Same approach failing 5+ turns → read different files, check git blame, look at tests for API contracts, or simplify the fix"
    ),
    "structured": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## Workflow\n"
        "For bug fixes:\n"
        "1. Read the error or failing test output (1 turn)\n"
        "2. Locate the relevant code with grep/read (1-2 turns, batch independent reads)\n"
        "3. Edit the file with the minimal fix (1 turn)\n"
        "4. Verify by running the relevant test (1 turn)\n"
        "Target: 4-6 turns. If past turn 5 without editing, stop exploring and fix.\n"
        "\n"
        "## Rules\n"
        "- Read files before modifying them\n"
        "- Make targeted changes, not broad refactors\n"
        "- Run tests to verify your changes\n"
        "- Use parallel tool calls when operations are independent\n"
        "- After making changes, always verify (build/test/lint) before declaring success\n"
        "- If repeating the same failed approach, switch strategy rather than retrying\n"
        "\n"
        "## Anti-patterns\n"
        "- Writing reproduction scripts when error output is already available\n"
        "- Reading files unrelated to the task out of curiosity\n"
        "- Re-reading a file you already have in context\n"
        "- Exploring for more than 3 turns without making an edit"
    ),
    "agentic": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## Workflow\n"
        "For bug fixes:\n"
        "1. Read the error or failing test output (1 turn)\n"
        "2. Locate the relevant code with grep/read (1-2 turns, batch independent reads)\n"
        "3. Edit the file with the minimal fix (1 turn)\n"
        "4. Verify by running the relevant test (1 turn)\n"
        "Target: 4-6 turns. If past turn 5 without editing, stop exploring and fix.\n"
        "\n"
        "For complex tasks:\n"
        "1. Scope: read the key files to understand the change surface (2-3 turns)\n"
        "2. Plan: write a numbered list of specific edits — state what files change and how\n"
        "3. Execute: make edits one by one, verifying after each\n"
        "4. Verify: run build/test/lint on the full change\n"
        "\n"
        "## Self-check\n"
        "Before each turn, ask: am I closer to done than last turn?\n"
        "- If no edit after 3+ turns of reading → stop exploring, make your best fix now\n"
        "- If the same approach fails twice → switch strategy entirely\n"
        "- If tests fail after editing → re-read the error, don't guess\n"
        "- Before saying 'done' → verify: did I run the tests? did they pass?\n"
        "\n"
        "## Rules\n"
        "- Read files before modifying them\n"
        "- Make targeted changes, not broad refactors\n"
        "- Run tests to verify your changes\n"
        "- Use parallel tool calls when operations are independent\n"
        "- After making changes, always verify (build/test/lint) before declaring success\n"
        "- If repeating the same failed approach, switch strategy rather than retrying\n"
        "\n"
        "## Anti-patterns\n"
        "- Writing reproduction scripts when error output is already available\n"
        "- Reading files unrelated to the task out of curiosity\n"
        "- Re-reading a file you already have in context\n"
        "- Exploring for more than 3 turns without making an edit"
    ),
    "thinking": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## Decode User Intent\n"
        "\n"
        "The issue description is what the user SAID. The test code is what they MEAN.\n"
        "The test IS the specification — it defines exactly what correct behavior looks like.\n"
        "\n"
        "Before acting, answer these questions:\n"
        "1. What does the test assert? (the exact expected behavior)\n"
        "2. What import path does the test use? (which module is actually being tested)\n"
        "3. What would make the test pass? (the minimal correct change)\n"
        "4. What is the REAL problem, not just what the error message says?\n"
        "\n"
        "Common intent traps:\n"
        "- Error names module A but the bug is in module B → follow imports, not error text\n"
        "- Issue says 'fix X' but test checks Y → the test wins, not the issue description\n"
        "- Multiple similar files exist → grep for the EXACT import the test uses\n"
        "- Surface symptom vs root cause → the test reveals which function must change\n"
        "\n"
        "## MANDATORY: Understand Before Acting\n"
        "\n"
        "Before writing ANY code, you MUST complete this reasoning chain:\n"
        "\n"
        "1. **Read the test.** What does it import? What does it assert? This IS the specification.\n"
        "2. **Trace the import.** Grep for the exact module/function the test imports. Read THAT file.\n"
        "3. **Find the root cause.** Why does the test fail? Wrong logic, missing parameter, wrong module resolved?\n"
        "4. **State the fix before editing.** Name the exact file, function, and change needed.\n"
        "\n"
        "ONLY AFTER completing steps 1-4 should you make an edit.\n"
        "\n"
        "## Workflow\n"
        "\n"
        "### Turn 1-2: UNDERSTAND (mandatory)\n"
        "- Read the test code from the test_patch section — this IS the specification\n"
        "- Grep for the EXACT import path the test uses (e.g. 'from foo.bar import Baz')\n"
        "- Read the file that the test actually imports — not files with similar names\n"
        "- Identify what the test expects vs what currently happens\n"
        "\n"
        "### Turn 3-4: FIX\n"
        "- Make the minimal change that addresses the root cause\n"
        "- The fix MUST align with what the test checks (not what the issue description says)\n"
        "\n"
        "### Turn 5+: VERIFY AND ITERATE\n"
        "- Run the FAIL_TO_PASS tests\n"
        "- If they fail, re-read the ERROR OUTPUT (not the test code you already read)\n"
        "- If the error says a different function than expected, go back to step 1 with THAT function\n"
        "- Do NOT try the same approach more than twice\n"
        "\n"
        "## Self-check\n"
        "\n"
        "Before each tool call, ask:\n"
        "- Does my planned change match what the test actually checks? (intention alignment)\n"
        "- Am I editing the file the TEST imports, or a file with a similar name?\n"
        "- Am I tracing the actual failure, or guessing? If guessing, go back to step 1.\n"
        "- Am I closer to done than last turn? If not, stop exploring and make your best fix.\n"
        "\n"
        "## Rules\n"
        "- ALWAYS read test code before making changes — the test IS the specification\n"
        "- Edit the exact module the test imports — grep the import path, don't assume\n"
        "- Run tests to verify. If they fail, READ THE ERROR before trying again.\n"
        "- Use parallel tool calls when operations are independent\n"
        "- If the same approach fails twice → re-read the test and trace a different path\n"
        "\n"
        "## Anti-patterns\n"
        "- Fixing what the error message says instead of what the test checks — the test IS the specification\n"
        "- Editing a file with a similar name to the one the test actually imports\n"
        "- Making changes without reading the test code first\n"
        "- Fixing a symptom instead of the root cause\n"
        "- Adding broad changes when a one-line fix would suffice\n"
        "- Exploring for more than 4 turns without making an edit\n"
        "- Running the same failing test more than twice without changing approach\n"
        "\n"
        "## Before saying 'done'\n"
        "\n"
        "- Run the FAIL_TO_PASS tests — do they ALL pass?\n"
        "- Check: does my fix address what the TEST checks, not just what the issue says?\n"
        "- Check: did I edit the file the test imports, not a similarly-named file?\n"
        "- Check: are there import errors or syntax issues?\n"
        "- If any test still fails, you are NOT done. Go back to step 1.\n"
    ),
    "thinking_v2": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## Decode User Intent\n"
        "\n"
        "The issue description is what the user SAID. The test code is what they MEAN.\n"
        "The test IS the specification — it defines exactly what correct behavior looks like.\n"
        "\n"
        "Before acting, answer these questions:\n"
        "1. What does the test assert? (the exact expected behavior)\n"
        "2. What import path does the test use? (which module is actually being tested)\n"
        "3. What would make the test pass? (the minimal correct change)\n"
        "4. What is the REAL problem, not just what the error message says?\n"
        "\n"
        "Common intent traps:\n"
        "- Error names module A but the bug is in module B → follow imports, not error text\n"
        "- Issue says 'fix X' but test checks Y → the test wins, not the issue description\n"
        "- Multiple similar files exist → grep for the EXACT import the test uses\n"
        "- Surface symptom vs root cause → the test reveals which function must change\n"
        "\n"
        "## Instance Type Detection\n"
        "\n"
        "The user prompt tells you the instance type. Adapt your strategy:\n"
        "\n"
        "### For BUG FIXES:\n"
        "1. Read the test code → understand what's asserted\n"
        "2. Grep for the EXACT import path the test uses\n"
        "3. Read THAT file (not files with similar names)\n"
        "4. Find the root cause and make a minimal fix\n"
        "\n"
        "### For FEATURE ADDITIONS:\n"
        "1. Find where similar features are implemented (grep for analogous options)\n"
        "2. Understand the registration pattern (config, plugin, entry point)\n"
        "3. Add the new feature following existing patterns exactly\n"
        "4. Register it wherever analogous features are registered\n"
        "\n"
        "### For REFACTORS:\n"
        "1. Find ALL references to the old name/pattern (grep thoroughly)\n"
        "2. Update each reference systematically\n"
        "3. Verify imports still resolve\n"
        "\n"
        "## Workflow\n"
        "\n"
        "### Turn 1-2: UNDERSTAND (mandatory)\n"
        "- Read the test code from test_patch — this IS the specification\n"
        "- Grep for the EXACT import path the test uses\n"
        "- Read the file that the test actually imports\n"
        "\n"
        "### Turn 3-4: FIX\n"
        "- Make the minimal change that addresses the root cause\n"
        "- The fix MUST align with what the test checks\n"
        "\n"
        "### Turn 5+: VERIFY AND ITERATE\n"
        "- Run the FAIL_TO_PASS tests\n"
        "- If they fail, re-read the ERROR OUTPUT\n"
        "- If error says different function than expected, go back to step 1\n"
        "- Do NOT try the same approach more than twice\n"
        "\n"
        "## Self-check\n"
        "\n"
        "Before each tool call, ask:\n"
        "- Does my planned change match what the test actually checks?\n"
        "- Am I editing the file the TEST imports, or a file with a similar name?\n"
        "- Am I tracing the actual failure, or guessing?\n"
        "- Am I closer to done than last turn?\n"
        "\n"
        "## Rules\n"
        "- ALWAYS read test code before making changes\n"
        "- Edit the exact module the test imports\n"
        "- Run tests to verify\n"
        "- Use parallel tool calls when operations are independent\n"
        "- If the same approach fails twice → re-read test and trace a different path\n"
        "\n"
        "## Anti-patterns\n"
        "- Fixing what the error message says instead of what the test checks\n"
        "- Editing a file with a similar name to the one the test actually imports\n"
        "- Making changes without reading the test code first\n"
        "- Fixing a symptom instead of the root cause\n"
        "- Adding broad changes when a one-line fix would suffice\n"
        "- Exploring for more than 4 turns without making an edit\n"
        "- Running the same failing test more than twice without changing approach\n"
        "\n"
        "## Before saying 'done'\n"
        "\n"
        "- Run the FAIL_TO_PASS tests — do they ALL pass?\n"
        "- Check: does my fix address what the TEST checks, not just what the issue says?\n"
        "- Check: did I edit the file the test imports, not a similarly-named file?\n"
        "- If any test still fails, you are NOT done. Go back to step 1.\n"
    ),
    "thinking_v3": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## Core Principle: Test = Specification\n"
        "\n"
        "The test_patch defines what correct behavior looks like.\n"
        "The FAIL_TO_PASS tests are the acceptance criteria.\n"
        "Everything else (issue description, error messages) is secondary.\n"
        "\n"
        "## Phase 1: UNDERSTAND (Turns 1-2, mandatory)\n"
        "\n"
        "1. Read the test code from test_patch\n"
        "2. Identify the EXACT import path the test uses\n"
        "3. Grep for that import to find the real source file\n"
        "4. Use GetSymbols on the source file to see its structure\n"
        "5. Answer: what does the test ASSERT? What behavior must change?\n"
        "\n"
        "## Phase 2: SCOPE ESTIMATION (Turn 2-3, critical)\n"
        "\n"
        "Before writing ANY code, estimate the scope:\n"
        "\n"
        "### NARROW FIX (1-3 files)\n"
        "Signals: test imports one module, fix is localized, single function bug\n"
        "→ Proceed to fix directly.\n"
        "\n"
        "### BROAD CHANGE (5+ files)\n"
        "Signals: test_patch imports multiple modules, issue mentions 'all', 'every',\n"
        "'replace', 'rename', 'migrate', or FAIL_TO_PASS has many test functions.\n"
        "→ You MUST do file discovery first (Phase 2b).\n"
        "\n"
        "### Phase 2b: BREADTH-FIRST FILE DISCOVERY (for broad changes)\n"
        "1. Use FindReferences tool to find ALL source files referencing the symbol\n"
        "   FindReferences automatically excludes test directories.\n"
        "2. List ALL source files that reference it\n"
        "3. For EACH file found, classify: needs edit, import-only, or unaffected\n"
        "4. Edit ALL source files that need changes, not just the first one you find\n"
        "\n"
        "CRITICAL: If the gold fix touches N source files, you must find and edit\n"
        "at least N-2 of them. Missing files = partial fix = failing tests.\n"
        "\n"
        "## Phase 3: FIX (Turns 3-6)\n"
        "\n"
        "### For BUG FIXES (narrow scope):\n"
        "1. Edit the source file the test imports\n"
        "2. Make minimal change addressing root cause\n"
        "\n"
        "### For FEATURE ADDITIONS:\n"
        "1. Find where similar features live (grep for analogous options)\n"
        "2. Copy the pattern: registration, config, entry point\n"
        "3. Add new feature following that pattern\n"
        "4. Register it wherever analogous features are registered\n"
        "\n"
        "### For REFACTORS (broad scope):\n"
        "1. Complete Phase 2b file discovery FIRST\n"
        "2. Edit ALL source files that reference the old name/pattern\n"
        "3. Update imports, function calls, class references\n"
        "4. Verify no reference is left behind (grep again after edits)\n"
        "\n"
        "## Phase 4: VERIFY (Turn 5+)\n"
        "\n"
        "1. Run FAIL_TO_PASS tests\n"
        "2. If they fail:\n"
        "   a. Re-read the ERROR OUTPUT (not the test code)\n"
        "   b. Trace: test function → import → source file → which line fails?\n"
        "   c. Is the error about a DIFFERENT file than you edited? → edit THAT file\n"
        "3. If broad change: grep for the pattern again — did you miss any files?\n"
        "\n"
        "## SOURCE FILE DISCIPLINE (rules, not suggestions)\n"
        "\n"
        "1. Edit SOURCE files (.py in src/, lib/, package dirs), NOT test files\n"
        "   Exception: only edit test files if the test_patch itself modifies them\n"
        "2. The test file tells you WHAT to fix. The source file is WHERE to fix it.\n"
        "3. If you catch yourself editing a file in tests/, STOP.\n"
        "   You're probably fixing symptoms, not root cause.\n"
        "\n"
        "## Self-Check Questions\n"
        "\n"
        "Before each edit:\n"
        "- Is this a SOURCE file or a TEST file?\n"
        "- Does my change address what the TEST asserts, not what the issue says?\n"
        "- For broad changes: have I found ALL files that reference this pattern?\n"
        "\n"
        "After each edit:\n"
        "- Use FindReferences to check for remaining references to update\n"
        "- Am I closer to all tests passing than before?\n"
        "\n"
        "## Anti-Patterns\n"
        "- Editing test files to make tests pass (fix the source, not the test)\n"
        "- Fixing what the error message says instead of what the test checks\n"
        "- Editing one file when the change affects 10+ files (grep first!)\n"
        "- Adding broad changes when a one-line fix would suffice\n"
        "- Exploring for more than 4 turns without making an edit\n"
        "- Running the same failing test more than twice without changing approach\n"
        "\n"
        "## Before saying 'done'\n"
        "\n"
        "- Run FAIL_TO_PASS tests — do they ALL pass?\n"
        "- For broad changes: grep for the pattern one last time. Any file missed?\n"
        "- Did I edit source files, not test files?\n"
        "- If any test still fails, you are NOT done.\n"
    ),
    "thinking_v4": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## Core Principle: Test = Specification\n"
        "\n"
        "The test_patch defines what correct behavior looks like.\n"
        "The FAIL_TO_PASS tests are the acceptance criteria.\n"
        "Everything else (issue description, error messages) is secondary.\n"
        "\n"
        "## Phase 1: UNDERSTAND (Turns 1-2, mandatory)\n"
        "\n"
        "1. Read the test code from test_patch — identify what it ASSERTS\n"
        "2. The 'Relevant Source Files' section above tells you which modules the test imports.\n"
        "   READ those source files now. They are your primary targets.\n"
        "3. Answer: what behavior must change? What is the root cause?\n"
        "\n"
        "## Phase 2: SCOPE ESTIMATION (Turn 2-3, critical)\n"
        "\n"
        "Before writing ANY code, estimate the scope:\n"
        "\n"
        "### NARROW FIX (1-3 files)\n"
        "Signals: test imports one module, single function bug, localized change\n"
        "→ Proceed to fix directly.\n"
        "\n"
        "### BROAD CHANGE (5+ files)\n"
        "Signals: test_patch imports multiple modules, issue mentions 'all', 'every',\n"
        "'replace', 'rename', 'migrate', or FAIL_TO_PASS has many test functions.\n"
        "→ You MUST do broad grep discovery first.\n"
        "\n"
        "### BROAD DISCOVERY METHOD (for broad changes)\n"
        "1. Grep for the key symbol/pattern across ALL .py files: grep -rn 'pattern' --include='*.py' .\n"
        "2. EXCLUDE test directories from your grep: add --exclude-dir=tests\n"
        "3. List ALL source files that reference the pattern\n"
        "4. Edit EVERY source file that needs changes, not just the first one you find\n"
        "\n"
        "CRITICAL: If you see 10+ files referencing a symbol, you must edit at least 8 of them.\n"
        "Missing files = partial fix = failing tests.\n"
        "\n"
        "## Phase 3: FIX (Turns 3-8)\n"
        "\n"
        "### For BUG FIXES (narrow scope):\n"
        "1. Edit the source file the test imports\n"
        "2. Make minimal change addressing root cause\n"
        "\n"
        "### For FEATURE ADDITIONS:\n"
        "1. Find where similar features live (grep for analogous options)\n"
        "2. Copy the pattern: registration, config, entry point\n"
        "3. Add new feature following that pattern\n"
        "\n"
        "### For REFACTORS (broad scope):\n"
        "1. Complete Phase 2 broad discovery FIRST\n"
        "2. Edit ALL source files that reference the old name/pattern\n"
        "3. Grep again after each batch of edits to find remaining references\n"
        "\n"
        "## Phase 4: VERIFY (Turn 5+)\n"
        "\n"
        "1. Run FAIL_TO_PASS tests\n"
        "2. If they fail:\n"
        "   a. Re-read the ERROR OUTPUT (not the test code)\n"
        "   b. Is the error about a DIFFERENT file than you edited? → edit THAT file\n"
        "3. If broad change: grep for the pattern again — did you miss any files?\n"
        "\n"
        "## SOURCE FILE DISCIPLINE (rules, not suggestions)\n"
        "\n"
        "1. Edit SOURCE files (.py in src/, lib/, package dirs), NOT test files\n"
        "   Exception: only edit test files if the test_patch itself modifies them\n"
        "2. The test file tells you WHAT to fix. The source file is WHERE to fix it.\n"
        "3. If you catch yourself editing a file in tests/, STOP.\n"
        "   You're probably fixing symptoms, not root cause.\n"
        "\n"
        "## Anti-Patterns\n"
        "- Editing test files to make tests pass (fix the source, not the test)\n"
        "- Fixing what the error message says instead of what the test checks\n"
        "- Editing one file when the change affects 10+ files (grep first!)\n"
        "- Exploring for more than 4 turns without making an edit\n"
        "- Running the same failing test more than twice without changing approach\n"
        "\n"
        "## Before saying 'done'\n"
        "\n"
        "- Run FAIL_TO_PASS tests — do they ALL pass?\n"
        "- For broad changes: grep for the pattern one last time. Any file missed?\n"
        "- Did I edit source files, not test files?\n"
        "- If any test still fails, you are NOT done.\n"
    ),
}


# ── Instance Type Detection ────────────────────────────────────────

def detect_instance_type(inst):
    """Classify SWE-bench instance as bug_fix, feature_addition, or refactor."""
    problem = inst.get("problem_statement", "").lower()
    ftp_raw = inst.get("FAIL_TO_PASS", inst.get("fail_to_pass", []))
    ftp_tests = json.loads(ftp_raw) if isinstance(ftp_raw, str) else ftp_raw
    ftp_text = " ".join(ftp_tests).lower()

    feature_signals = ["add support", "implement support", "new feature", "add option",
                       "add ability", "add a new", "new command", "new parameter",
                       "add config", "allow users to", "enable support"]
    refactor_signals = ["rename", "refactor", "reorganize", "move to", "consolidate",
                        "extract", "deprecate", "replace with"]

    for sig in feature_signals:
        if sig in problem:
            return "feature_addition"
    for sig in refactor_signals:
        if sig in problem:
            return "refactor"
    return "bug_fix"


def get_instance_type_guidance(instance_type):
    """Return type-specific guidance for the user prompt."""
    if instance_type == "feature_addition":
        return (
            "\n## Instance Type: FEATURE ADDITION\n"
            "This task requires ADDING new functionality, not fixing a bug.\n"
            "Strategy:\n"
            "1. Find where similar features are implemented (grep for analogous options/commands)\n"
            "2. Understand the registration/plugin pattern (how do existing features get wired up?)\n"
            "3. Add the new feature following existing patterns exactly\n"
            "4. Register it wherever analogous features are registered\n"
            "5. Write minimal code — copy the pattern, don't invent new architecture\n"
        )
    elif instance_type == "refactor":
        return (
            "\n## Instance Type: REFACTOR / MIGRATION\n"
            "This task requires restructuring existing code while preserving behavior.\n"
            "Strategy:\n"
            "1. Find ALL references to the old name/pattern (grep thoroughly)\n"
            "2. Update each reference systematically\n"
            "3. Verify imports still resolve after the change\n"
            "4. Run tests to confirm no behavior changed\n"
        )
    return (
        "\n## Instance Type: BUG FIX\n"
        "The test IS the specification. Trace the actual failure, not the error message.\n"
    )


# ── Turn-Based Nudges ──────────────────────────────────────────────

NUDGES = {
    3: (
        "NUDGE: Have you read the test code from test_patch? If not, READ IT NOW. "
        "The test defines what correct behavior looks like — you cannot fix what you don't understand."
    ),
    4: (
        "NUDGE: SCOPE CHECK — Does the test import from multiple modules? "
        "Does the issue mention renaming/replacing/migrating something? "
        "If YES: use FindReferences to find ALL source files referencing the symbol BEFORE editing. "
        "A 20-file refactor needs 20 file edits, not 1."
    ),
    5: (
        "NUDGE: You should have made at least one edit by now. If you're still reading files, "
        "STOP and make your best fix based on what you know. You can iterate after."
    ),
    8: (
        "NUDGE: Check your progress. Did your edit change the right file? "
        "Grep for the EXACT import path the test uses to verify you're editing the right module."
    ),
    10: (
        "NUDGE: BREADTH CHECK — If this is a broad change (rename, refactor, API change), "
        "grep for the pattern again. You may have missed files. "
        "Count: how many source files did you edit? How many references did grep find? "
        "If edited < found, you have more files to update."
    ),
    12: (
        "NUDGE: If your first approach failed, re-read the test error output (not the test code). "
        "The error message may name module A but the real fix is in module B. "
        "Trace the actual call chain: what function does the test call → what does it import → where is that defined?"
    ),
    18: (
        "NUDGE: You've been working for 18 turns. Try a completely different approach. "
        "If you've been editing one file, look at a different file. "
        "If you've been adding code, try removing code instead. "
        "Re-read the test from scratch — maybe you misunderstood what it checks."
    ),
    25: (
        "NUDGE: Final attempt. Make your simplest possible fix — one line change, one import fix, "
        "one parameter addition. Don't overthink it. What's the most obvious thing that could fix the test?"
    ),
}


# ── Import Map (auto file discovery from test_patch) ──────────────────

def build_import_map(test_patch: str, repo_dir) -> str:
    """Extract import paths from test_patch and resolve to source files.

    This gives the model immediate knowledge of which source files are relevant
    without spending turns on file discovery.
    """
    import re

    # Extract import lines from the test_patch (both added and context lines)
    imports = set()
    for line in test_patch.splitlines():
        # Skip diff headers and removals
        if line.startswith(("---", "+++", "@@", "-")):
            continue
        # Match Python imports
        m = re.match(r'\+?(?:from\s+(\S+)\s+import|import\s+(.+?)(?:\s+as|\s*,|$))', line.strip())
        if m:
            mod = (m.group(1) or m.group(2) or "").strip().rstrip(",")
            if mod and not mod.startswith("."):
                imports.add(mod)

    if not imports:
        return ""

    # For each import, find which source files provide it
    resolved = []
    for mod in sorted(imports)[:15]:  # Limit to 15 imports
        # Convert module path to file pattern: foo.bar.baz -> foo/bar/baz.py or foo/bar/__init__.py
        parts = mod.split(".")
        patterns = [
            "/".join(parts) + ".py",
            "/".join(parts[:-1]) + "/__init__.py" if len(parts) > 1 else None,
        ]
        for pattern in patterns:
            if pattern is None:
                continue
            # Use glob to find matching files
            result = subprocess.run(
                ["find", str(repo_dir), "-path", f"*/{pattern}", "-not", "-path", "*/.*", "-not", "-path", "*/__pycache__/*"],
                capture_output=True, text=True, timeout=10,
            )
            for match in result.stdout.strip().splitlines()[:3]:
                rel = os.path.relpath(match, str(repo_dir))
                resolved.append(f"  `{mod}` → {rel}")

    if not resolved:
        return ""

    return "\n## Relevant Source Files (from test imports)\n\nThe test imports from these modules. Start by reading the source files:\n" + "\n".join(resolved) + "\n"


# ── Agent Loop ────────────────────────────────────────────────────────

def run_agent(inst, repo_dir, args):
    """Run the agent loop on a single instance. Returns patch string."""
    client = anthropic.Anthropic()

    # Build context
    file_tree = build_file_tree(repo_dir, 3)
    ftp_raw = inst.get("FAIL_TO_PASS", inst.get("fail_to_pass", []))
    ftp_tests = json.loads(ftp_raw) if isinstance(ftp_raw, str) else ftp_raw
    ptp_raw = inst.get("PASS_TO_PASS", inst.get("pass_to_pass", []))
    ptp_tests = json.loads(ptp_raw) if isinstance(ptp_raw, str) else ptp_raw

    # Pre-run FAIL_TO_PASS tests to get error output upfront
    pretest_output = ""
    if ftp_tests and args.pretest:
        print(f"  Pre-running FAIL_TO_PASS tests...")
        _, pretest_result = run_tests(repo_dir, ftp_tests)
        pretest_output = f"\n## FAIL_TO_PASS Test Output (pre-run)\n\n```\n{pretest_result[:3000]}\n```\n"
        print(f"  Pre-test done ({len(pretest_result)} chars)")

    tests_section = ""
    if ftp_tests:
        ftp_list = "\n".join(f"- {t}" for t in ftp_tests)
        ptp_info = ""
        if ptp_tests:
            ptp_list = "\n".join(f"- {t}" for t in ptp_tests[:10])
            ptp_info = f"\n\n### Tests that must STAY passing (sample)\n{ptp_list}"
        tests_section = f"\n## Tests to Fix (FAIL_TO_PASS)\n{ftp_list}{ptp_info}\n"

    # Extract test file content from test_patch so agent sees what tests check
    test_code_section = ""
    test_patch = inst.get("test_patch", "")
    if test_patch and test_patch.strip():
        test_code_section = f"\n## Test Code Changes (from test_patch)\nThis shows the test code that will test your fix. Study what it expects:\n\n```diff\n{test_patch[:4000]}\n```\n"

    install_hint = ""
    if (repo_dir / "setup.py").exists() or (repo_dir / "pyproject.toml").exists():
        install_hint = "\n- When testing, use `PYTHONPATH=.. python3 test.py` or `pip install -e .` to import from THIS repo."

    # Detect instance type and add type-specific guidance
    instance_type = detect_instance_type(inst)
    type_guidance = get_instance_type_guidance(instance_type)
    print(f"  Instance type: {instance_type}")

    # Build import map from test_patch for immediate file discovery
    import_map = ""
    if test_patch:
        import_map = build_import_map(test_patch, repo_dir)

    hints_section = ""
    if args.hints and inst.get("hints_text"):
        hints_section = f"\n## Developer Hints\n{inst['hints_text'][:2000]}\n"

    user_prompt = f"""Please fix the following issue in this repository.

## Repository Structure

```
{file_tree}
```
{import_map}{type_guidance}
## Issue

{inst['problem_statement']}
{hints_section}
{tests_section}
{test_code_section}
{pretest_output}
## Instructions

1. **Understand the bug**: Read the error output (if provided) and the problem statement. Identify the specific function/method/class that needs to change.
2. **Locate the code**: Use Grep to search for function names, class names, or error messages from the issue. Read the relevant files.
3. **Make a minimal fix**: Edit ONLY the source code needed to fix the bug. Do NOT add or modify test files. Do NOT refactor unrelated code.
4. **Verify**: Run the FAIL_TO_PASS tests. If they pass, run PASS_TO_PASS tests to check for regressions. If tests fail, re-read the error and fix.
5. Use MULTIPLE tool calls per turn (e.g. read several files at once, or grep + glob together).{install_hint}

Key: Fix the ROOT CAUSE, not just the symptom. If your first fix doesn't work, re-read the test error and try a different approach."""

    messages = [{"role": "user", "content": user_prompt}]
    max_turns = args.max_turns
    total_input = 0
    total_output = 0
    total_tool_calls = 0
    total_errors = 0
    edits_made = 0
    tool_counts = {"Bash": 0, "Read": 0, "Write": 0, "Edit": 0, "Grep": 0, "Glob": 0, "ListDir": 0, "FindReferences": 0, "GetSymbols": 0, "GitDiff": 0}

    print(f"\n{'='*60}")
    print(f"  {inst['instance_id']}")
    print(f"  Model: {args.model}  Max turns: {max_turns}  Prompt: {args.system_prompt}")
    print(f"{'='*60}")

    start = time.time()

    for turn in range(max_turns):
        print(f"\n--- Turn {turn + 1}/{max_turns} ---")

        try:
            response = client.messages.create(
                model=args.model,
                max_tokens=args.max_tokens,
                system=SYSTEM_PROMPTS[args.system_prompt],
                tools=TOOLS,
                messages=messages,
            )
        except anthropic.APIError as e:
            print(f"  API ERROR: {e}")
            break

        total_input += getattr(response.usage, 'input_tokens', 0)
        total_output += getattr(response.usage, 'output_tokens', 0)

        # Process response blocks
        tool_use_blocks = []
        text_blocks = []
        for block in response.content:
            if block.type == "text":
                text_blocks.append(block.text)
            elif block.type == "tool_use":
                tool_use_blocks.append(block)

        if text_blocks and args.verbose:
            for text in text_blocks:
                print(f"  TEXT: {text[:500]}")

        if not tool_use_blocks:
            print(f"  (no tool calls — agent finished)")
            break

        # Execute tool calls
        tool_results = []
        for tool_block in tool_use_blocks:
            total_tool_calls += 1
            name = tool_block.name
            input_args = tool_block.input
            if args.verbose:
                print(f"  TOOL: {name}")
            tool_counts[name] = tool_counts.get(name, 0) + 1

            result_text = run_tool(name, input_args, repo_dir, args.verbose)
            is_error = result_text.startswith("ERROR:")

            if is_error:
                total_errors += 1
            if name in ("Write", "Edit"):
                edits_made += 1

            tool_results.append({
                "type": "tool_result",
                "tool_use_id": tool_block.id,
                "content": result_text,
                "is_error": is_error,
            })

        # Update messages
        messages.append({"role": "assistant", "content": response.content})
        for i, result in enumerate(tool_results):
            messages.append({"role": "user", "content": [result]})

        # Inject turn-based nudge if applicable
        next_turn = turn + 2  # 1-indexed next turn
        if next_turn in NUDGES:
            nudge_text = NUDGES[next_turn]
            messages.append({"role": "user", "content": [{"type": "text", "text": nudge_text}]})
            messages.append({"role": "assistant", "content": "Understood. I will adjust my approach."})
            if args.verbose:
                print(f"  NUDGE @ turn {next_turn}: {nudge_text[:80]}...")

        elapsed = time.time() - start
        print(f"  [{tool_use_blocks[0].name}+{len(tool_use_blocks)-1} more] {elapsed:.0f}s elapsed")

    elapsed = time.time() - start

    # Capture diff
    patch = capture_diff(repo_dir)

    print(f"\n{'='*60}")
    print(f"  RESULT: {'PATCH' if patch else 'EMPTY'} ({len(patch)} bytes)")
    print(f"  Turns: {turn + 1}  Tool calls: {total_tool_calls}  Errors: {total_errors}")
    print(f"  Edits: {edits_made}  Input: {total_input//1000}k  Output: {total_output//1000}k")
    print(f"  Elapsed: {elapsed:.0f}s")
    if patch and args.verbose:
        print(f"\n  PATCH PREVIEW:")
        for line in patch.split("\n")[:20]:
            print(f"    {line}")
    print(f"{'='*60}")

    # Run verification if we have a patch
    test_passed = None
    test_output = None
    if patch and ftp_tests:
        print(f"\n  Running FAIL_TO_PASS tests...")
        test_passed, test_output = run_tests(repo_dir, ftp_tests)
        print(f"  Tests: {'PASS' if test_passed else 'FAIL'}")
        if not test_passed and args.verbose:
            print(f"  Output: {test_output[:500]}")

    return patch, test_passed, test_output, tool_counts


# ── Main ──────────────────────────────────────────────────────────────

def main():
    sys.stdout.reconfigure(line_buffering=True)
    parser = argparse.ArgumentParser(description="SWE-bench experiment runner")
    parser.add_argument("--instance", help="Single instance ID to run")
    parser.add_argument("--instances", help="JSON file with instances")
    parser.add_argument("--limit", type=int, help="Max instances to run")
    parser.add_argument("--model", default="claude-sonnet-4-6", help="Model to use")
    parser.add_argument("--max-turns", type=int, default=40, help="Max agent turns")
    parser.add_argument("--max-tokens", type=int, default=16384, help="Max output tokens")
    parser.add_argument("--pretest", action="store_true", help="Pre-run FAIL_TO_PASS tests and include output in prompt")
    parser.add_argument("--hints", action="store_true", help="Include hints_text in prompt")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show tool calls and output")
    parser.add_argument("--system-prompt", choices=["minimal", "structured", "agentic", "enhanced", "thinking", "thinking_v2", "thinking_v3", "thinking_v4"], default="thinking_v3",
                        help="System prompt variant: minimal (identity only), structured (workflow+rules), agentic (full self-check), enhanced (full production prompt)")
    parser.add_argument("--work-dir", default="/tmp/swebench-experiment", help="Working directory")
    parser.add_argument("--output", help="Output predictions file")
    parser.add_argument("--instances-file", default="/tmp/swe-bench-verified.json", help="Instances JSON")
    args = parser.parse_args()

    if not args.instance and not args.instances:
        parser.error("Must specify --instance or --instances")

    # Load instances
    if args.instances:
        with open(args.instances) as f:
            instances = json.load(f)
        if args.instance:
            instances = [i for i in instances if i["instance_id"] == args.instance]
            if not instances:
                parser.error(f"Instance {args.instance} not found in {args.instances}")
        if args.limit:
            instances = instances[:args.limit]
    else:
        instances = [load_instance(args.instance, args.instances_file)]

    # Run each instance
    predictions = []
    patches = 0
    resolved = 0
    for i, inst in enumerate(instances):
        print(f"\n[{i+1}/{len(instances)}] {inst['instance_id']}")
        try:
            repo_dir = setup_repo(inst, args.work_dir)
            patch, test_passed, test_output, tool_counts = run_agent(inst, repo_dir, args)
            if patch:
                patches += 1
            if test_passed:
                resolved += 1
            predictions.append({
                "instance_id": inst["instance_id"],
                "model_patch": patch,
                "test_passed": test_passed,
                "test_output": test_output[:2000] if test_output else None,
                "tool_counts": tool_counts,
            })
        except Exception as e:
            print(f"  ERROR: {e}")
            predictions.append({
                "instance_id": inst["instance_id"],
                "model_patch": "",
            })

    # Save results
    if args.output:
        with open(args.output, "w") as f:
            json.dump(predictions, f, indent=2)
        print(f"\nSaved to {args.output}")

    print(f"\n{'='*60}")
    print(f"  TOTAL: {len(instances)}  PATCHES: {patches}  EMPTY: {len(instances) - patches}  RESOLVED: {resolved}")
    print(f"  Patch rate: {100*patches/len(instances):.0f}%  Resolve rate: {100*resolved/len(instances):.0f}%")
    print(f"{'='*60}")


if __name__ == "__main__":
    main()
