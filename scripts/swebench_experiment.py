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
        "description": "Replace exact text in a SOURCE file. Will BLOCK edits to test files (paths containing /tests/, /test_, conftest.py). The test tells you WHAT to fix — source files are WHERE to fix. Use Read first to get exact content.",
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
        "description": "Search for a pattern in files. CRITICAL for scope discovery: grep for key symbols to find ALL source files that need changes. Count the results — if 5+ source files match, this is a broad refactor requiring edits to all of them. Use include='*.py' and exclude test directories.",
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
        "description": "Write or update a todo list to track multi-step fixes. REQUIRED for all fixes: list EVERY source file that needs changes as a separate todo item. Mark each 'completed' after editing. This IS your implementation plan.",
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
    {
        "name": "BatchEdit",
        "description": "Apply the same text replacement across MULTIPLE files at once. PERFECT for broad refactors where the same change is needed in many files (e.g., renaming a function, updating imports, replacing a pattern). Much more efficient than editing files one by one. Automatically skips test files — only edits source files.",
        "input_schema": {
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of file paths to edit (relative to repo root)",
                },
                "old_text": {"type": "string", "description": "Exact text to find in each file"},
                "new_text": {"type": "string", "description": "Replacement text"},
            },
            "required": ["files", "old_text", "new_text"],
        },
    },
    {
        "name": "ClassifyFiles",
        "description": "Classify a list of file paths as SOURCE files (edit these) or TEST files (do NOT edit). Use BEFORE editing to verify you're targeting the right files. Run this on your grep results.",
        "input_schema": {
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "File paths to classify",
                },
            },
            "required": ["files"],
        },
    },
    {
        "name": "SourceDirs",
        "description": "List the auto-detected source code directories in this project. Use to understand the project layout before editing.",
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
]


# ── Tool Execution ────────────────────────────────────────────────────

def is_test_file(path: str) -> bool:
    """Check if a file path is a test file."""
    lower = path.lower()
    # Common test directory/file patterns
    test_indicators = [
        "/tests/", "/test/", "/__tests__/", "/testing/",
        "/test_", "\\tests\\", "\\test\\",
    ]
    for indicator in test_indicators:
        if indicator in lower:
            return True
    # File-level patterns
    basename = lower.split("/")[-1].split("\\")[-1]
    if basename.startswith("test_") or basename.endswith("_test.py") or basename.endswith("test.py"):
        return True
    if basename == "conftest.py":
        return True
    return False


def detect_source_dirs(repo_dir) -> list[str]:
    """Auto-detect source directories by looking for Python packages with __init__.py
    that are NOT test directories."""
    import os
    source_dirs = []
    for entry in sorted(os.listdir(repo_dir)):
        entry_path = os.path.join(repo_dir, entry)
        if not os.path.isdir(entry_path):
            continue
        # Skip hidden, test, cache, venv dirs
        if entry.startswith((".", "_")) or "test" in entry.lower():
            continue
        if entry in ("venv", "env", "node_modules", "__pycache__", "build", "dist", ".git"):
            continue
        # Check if it has __init__.py (Python package)
        init_path = os.path.join(entry_path, "__init__.py")
        if os.path.exists(init_path):
            source_dirs.append(entry)
    return source_dirs


def run_tool(name, input_args, repo_dir, verbose=False, system_prompt="thinking_v7"):
    """Execute a tool call and return the result string."""
    if name == "Bash":
        cmd = input_args["command"]
        timeout = input_args.get("timeout", 120)
        if verbose:
            print(f"    $ {cmd}")
        # v7: Detect file-modifying commands that target test files
        # (sed -i, awk with redirect, python -c with file write, etc.)
        import re as _re
        file_mod_patterns = [
            r'sed\s+.*-i',                    # sed -i
            r'awk\s+.*>\s*',                  # awk with redirect
            r'python.*-c.*open\(',            # python one-liner writing files
            r'perl\s+.*-i',                   # perl -i
            r'patch\s+-p',                    # patch command
            r'cp\s+',                         # copy
            r'mv\s+',                         # move
        ]
        is_file_mod = any(_re.search(p, cmd) for p in file_mod_patterns)
        if is_file_mod:
            # Extract target file paths from the command
            for test_path in _re.findall(r'[\w/.]+\.py', cmd):
                if is_test_file(test_path):
                    source_dirs = detect_source_dirs(repo_dir)
                    source_hint = ", ".join(source_dirs[:5]) if source_dirs else "lib/, src/"
                    return (
                        f"WARNING: This command modifies TEST file: {test_path}\n"
                        f"You must edit SOURCE files (in {source_hint}/), not test files.\n"
                        f"The test tells you WHAT to fix. Source files are WHERE to fix."
                    )
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
        # v7-only: Warn when writing test files
        if system_prompt == "thinking_v7" and is_test_file(path) and not input_args.get("force"):
            source_dirs = detect_source_dirs(repo_dir)
            source_hint = ", ".join(source_dirs[:5]) if source_dirs else "lib/, src/"
            return (
                f"WARNING: {path} is a TEST file. You should edit SOURCE files, not test files.\n"
                f"Source directories: {source_hint}/\n"
                f"To proceed anyway, re-send with 'force: true'."
            )
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
        # v7-only: Warn when editing test files — model should edit SOURCE files
        if system_prompt == "thinking_v7" and is_test_file(path):
            source_dirs = detect_source_dirs(repo_dir)
            source_hint = ", ".join(source_dirs[:5]) if source_dirs else "lib/, src/"
            return (
                f"WARNING: {path} is a TEST file. You should edit SOURCE files, not test files.\n"
                f"The test tells you WHAT to fix. Source files (in {source_hint}/) are WHERE to fix.\n"
                f"If you need to understand the test, use Read instead of Edit.\n"
                f"To proceed anyway, re-send this Edit with 'force: true' in the arguments."
            )
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
            return f"Edited {path} (source file)"
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
            if not result.stdout:
                return "(no matches)"
            lines = result.stdout.splitlines()
            # Separate source files from test files
            source_lines = [l for l in lines if "/test" not in l and "/__pycache__" not in l]
            source_files = set(l.split(":")[0] for l in source_lines if ":" in l)
            # Add scope summary header
            header = f"Found {len(source_lines)} matches in {len(source_files)} source files"
            if len(source_lines) < len(lines):
                test_count = len(lines) - len(source_lines)
                header += f" (+ {test_count} matches in test files)"
            if len(source_files) >= 10:
                header += " — BROAD REFACTOR: you MUST edit all these files"
            elif len(source_files) >= 5:
                header += " — ensure you edit ALL source files above"
            output = header + "\n\n" + "\n".join(source_lines[:300])
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

    elif name == "BatchEdit":
        files = input_args.get("files", [])
        old_text = input_args.get("old_text", "")
        new_text = input_args.get("new_text", "")
        if verbose:
            print(f"    BATCHEDIT '{old_text[:40]}...' → '{new_text[:40]}...' in {len(files)} files")
        # v7: Filter out test files with warning
        test_files = [f for f in files if is_test_file(f)]
        source_files = [f for f in files if not is_test_file(f)]
        if test_files and not source_files:
            return (
                f"WARNING: ALL {len(test_files)} target files are TEST files. "
                f"You must edit SOURCE files, not test files.\n"
                f"Test files: {', '.join(test_files[:5])}\n"
                f"Grep for the pattern in source directories to find the correct files."
            )
        results = []
        changed = 0
        edit_files = source_files  # Only edit source files
        for fpath in edit_files:
            if not full_path.exists():
                results.append(f"  SKIP {fpath}: file not found")
                continue
            content = full_path.read_text(errors="replace")
            if old_text not in content:
                results.append(f"  SKIP {fpath}: pattern not found")
                continue
            new_content = content.replace(old_text, new_text)
            full_path.write_text(new_content)
            count = content.count(old_text)
            changed += 1
            results.append(f"  OK   {fpath}: {count} replacement(s)")
        summary = f"BatchEdit: {changed}/{len(edit_files)} source files changed"
        if test_files:
            summary += f" (skipped {len(test_files)} test files)"
        return summary + "\n" + "\n".join(results)

    elif name == "ClassifyFiles":
        """Tell the model which files are source vs test."""
        files = input_args.get("files", [])
        if verbose:
            print(f"    CLASSIFY {len(files)} files")
        source = []
        test = []
        for f in files:
            if is_test_file(f):
                test.append(f)
            else:
                source.append(f)
        result_parts = []
        if source:
            result_parts.append(f"SOURCE files ({len(source)}):\n" + "\n".join(f"  {f}" for f in source))
        if test:
            result_parts.append(f"TEST files ({len(test)}) — do NOT edit:\n" + "\n".join(f"  {f}" for f in test))
        return "\n\n".join(result_parts)

    elif name == "SourceDirs":
        """List the source directories in this project."""
        if verbose:
            print("    SOURCEDIRS")
        dirs = detect_source_dirs(repo_dir)
        if not dirs:
            return "Could not auto-detect source directories. Look for directories with __init__.py."
        return "Source directories: " + ", ".join(dirs)

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
    """Clone and checkout the instance's repo. Returns (repo_path, venv_python)."""
    import shutil
    inst_dir = Path(work_dir) / inst["instance_id"]
    clone_dir = inst_dir / "repo"
    venv_dir = inst_dir / "venv"

    # Use Python 3.11 for compatibility with older packages
    base_python = shutil.which("python3.11") or shutil.which("python3.12") or "python3"

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

    # Create venv with compatible Python if not exists
    venv_python = venv_dir / "bin" / "python"
    if not venv_python.exists():
        print(f"  Creating venv with {base_python}...")
        subprocess.run(
            [base_python, "-m", "venv", str(venv_dir)],
            capture_output=True, timeout=60,
        )
        # Install base deps
        subprocess.run(
            [str(venv_python), "-m", "pip", "install", "--quiet", "pip", "pytest"],
            capture_output=True, timeout=120,
        )
        # Install package in development mode if setup.py/pyproject.toml exists
        if (clone_dir / "setup.py").exists() or (clone_dir / "pyproject.toml").exists():
            print(f"  Installing package...")
            r = subprocess.run(
                [str(venv_python), "-m", "pip", "install", "--quiet", "-e", str(clone_dir)],
                capture_output=True, timeout=300,
            )
            if r.returncode != 0:
                print(f"  Warning: pip install failed: {r.stderr[:200]}")

    return clone_dir, str(venv_python)


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
    # Stage all changes (including new files) to capture them in diff
    subprocess.run(["git", "add", "-A"], cwd=repo_dir, capture_output=True)
    result = subprocess.run(
        ["git", "diff", "--cached"], cwd=repo_dir, capture_output=True, text=True,
    )
    return result.stdout


def run_tests(repo_dir, test_names):
    """Run specific test names and return (passed, output)."""
    if not test_names:
        return True, "(no tests)"

    # Use Python 3.11 for compatibility with older packages
    import shutil
    python = shutil.which("python3.11") or shutil.which("python3.12") or "python3"

    # Detect test runner
    has_pytest = (repo_dir / "pytest.ini").exists() or (repo_dir / "pyproject.toml").exists()
    has_django = (repo_dir / "tests" / "runtests.py").exists()

    results = []
    all_passed = True
    for test in test_names:
        if has_django:
            # Extract module from Django test format
            module = test.split("(")[0].rsplit(".", 1)[0] if "(" in test else test
            cmd = f"{python} tests/runtests.py {module} --verbosity=2 2>&1"
        else:
            # Install package if needed, then run test
            cmd = (
                f"cd {repo_dir} && "
                f"PYTHONPATH={repo_dir}:/tmp/swebench-experiment "
                f"{python} -m pytest {test} -x --tb=short --no-header -q 2>&1"
            )

        result = subprocess.run(
            cmd, shell=True, cwd=repo_dir, capture_output=True, text=True, timeout=120,
        )
        passed = result.returncode == 0
        all_passed = all_passed and passed
        results.append(f"{'PASS' if passed else 'FAIL'}: {test}")
        if not passed:
            output = result.stdout or result.stderr
            results.append(output[-2000:] if output else "(no output)")

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
    "thinking_v5": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## CRITICAL: TWO-PHASE APPROACH\n"
        "\n"
        "You MUST follow this two-phase approach. Skipping Phase 1 causes failures.\n"
        "\n"
        "## PHASE 1: DISCOVER (Turns 1-4, NO EDITS ALLOWED)\n"
        "\n"
        "In this phase, you ONLY read and grep. Do NOT use Edit or Write.\n"
        "\n"
        "1. Read the test_patch code — understand what it ASSERTS\n"
        "2. Read the 'Relevant Source Files' listed above — these are your targets\n"
        "3. Grep for the key symbol/pattern to find ALL affected files:\n"
        "   grep -rn 'symbol' --include='*.py' . | grep -v test | grep -v __pycache__\n"
        "4. Write a FILE CHECKLIST using TodoWrite:\n"
        "   - List EVERY source file that needs changes\n"
        "   - Mark each as 'pending'\n"
        "   - This checklist IS your plan. Do not skip it.\n"
        "\n"
        "SCOPE RULE: If grep finds references in 10+ source files, this is a BROAD change.\n"
        "You MUST find and edit at least 80% of them.\n"
        "\n"
        "## PHASE 2: EXECUTE (Turns 5+, edit files and check them off)\n"
        "\n"
        "1. For each file in your checklist:\n"
        "   a. Read the file (if not already read)\n"
        "   b. Make the minimal edit needed\n"
        "   c. Update TodoWrite: mark it 'completed'\n"
        "2. After editing ALL files, run FAIL_TO_PASS tests\n"
        "3. If tests fail, re-read error output and fix. Check: did you miss a file?\n"
        "\n"
        "## SOURCE FILE DISCIPLINE\n"
        "\n"
        "1. Edit SOURCE files (.py in src/, lib/, package dirs), NOT test files\n"
        "2. The test tells you WHAT to fix. Source files are WHERE to fix.\n"
        "3. If you're editing a file in tests/, STOP — you're fixing symptoms.\n"
        "\n"
        "## ANTI-PATTERNS (common failures)\n"
        "- Starting to edit before discovering all affected files\n"
        "- Editing test files instead of source files\n"
        "- Fixing the error message instead of the root cause\n"
        "- Giving up after editing 2-3 files when 15+ need changes\n"
        "- Re-reading the test code instead of reading the error output\n"
        "\n"
        "## BEFORE SAYING 'DONE'\n"
        "- ALL items in your TodoWrite checklist are 'completed'\n"
        "- FAIL_TO_PASS tests all pass\n"
        "- Grep one last time: any file referencing the pattern that you missed?\n"
    ),

    # v6: Improved scope handling, breadth-first discovery, batch editing
    "thinking_v6": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## CRITICAL: THREE-STEP METHOD\n"
        "\n"
        "You MUST follow all three steps. Skipping Step 1 is the #1 cause of failure.\n"
        "\n"
        "## STEP 1: SCOPE DISCOVERY (Turns 1-3, NO EDITS)\n"
        "\n"
        "Goal: Count EXACTLY how many source files need changes.\n"
        "\n"
        "1. Read the test_patch — understand what the test ASSERTS\n"
        "2. Read the 'Relevant Source Files' listed above — these are primary targets\n"
        "3. Grep for the core pattern across ALL .py source files:\n"
        "   Bash: grep -rn 'PATTERN' --include='*.py' . | grep -v '/test' | grep -v __pycache__ | grep -v '.pyc'\n"
        "4. COUNT the grep results. This count = your TARGET file count.\n"
        "5. Write TodoWrite checklist with EVERY file from grep + import map.\n"
        "\n"
        "SCOPE DETECTION:\n"
        "- 1-3 files = NARROW fix → focused edits, minimal discovery needed\n"
        "- 4-9 files = MEDIUM change → read each file, understand context, edit carefully\n"
        "- 10+ files = BROAD refactor → batch edit with same pattern, don't overthink each file\n"
        "\n"
        "For BROAD refactors (10+ files):\n"
        "- The change is usually the SAME transformation applied to many files\n"
        "- Don't waste turns reading each file deeply — read 2-3 to understand the pattern, then batch-edit the rest\n"
        "- Use BatchEdit tool to apply the same replacement to multiple files in ONE call\n"
        "- Target: cover all files in 2-3 BatchEdit calls to stay within turn budget\n"
        "\n"
        "## STEP 2: EDIT SOURCE FILES (Turns 4-20)\n"
        "\n"
        "Rules:\n"
        "1. Edit SOURCE files ONLY (.py in src/, lib/, package dirs). NEVER edit test files.\n"
        "2. The test tells you WHAT to fix. Source files are WHERE to fix.\n"
        "3. If you're editing a file in tests/, STOP — you're fixing symptoms not causes.\n"
        "4. Mark each file 'completed' in TodoWrite after editing.\n"
        "5. For BROAD refactors: batch 5-8 file edits per turn.\n"
        "\n"
        "## STEP 3: VERIFY (Turns 20-30)\n"
        "\n"
        "1. Run FAIL_TO_PASS tests\n"
        "2. If tests fail:\n"
        "   a. Read the ACTUAL error output (not the test code)\n"
        "   b. Trace: what function does the test call → what does it import → where is that defined?\n"
        "   c. Grep again — did you miss any files that reference the pattern?\n"
        "   d. Fix and re-run\n"
        "\n"
        "## ANTI-PATTERNS (these cause 90% of failures)\n"
        "- Editing test files instead of source files (MOST COMMON FAILURE)\n"
        "- Giving up after editing 2-3 files when 15+ need changes\n"
        "- Fixing error messages instead of root causes\n"
        "- Not grepping broadly enough — if Grep reports 20+ source files but you only found 3, grep again with a broader pattern\n"
        "- Re-reading test code instead of reading test ERROR output\n"
        "- Editing __init__.py re-exports before editing the actual implementation\n"
        "\n"
        "## GREP STRATEGIES FOR FILE DISCOVERY\n"
        "If your first grep finds too few files, try broader patterns:\n"
        "- Class name → also grep for the module it's imported from\n"
        "- Function name → also grep for callers and importers\n"
        "- Error message text → grep for where it's raised\n"
        "- Import path → grep for both 'from X import' and 'import X'\n"
        "\n"
        "## BEFORE SAYING 'DONE'\n"
        "- TodoWrite: ALL items completed\n"
        "- FAIL_TO_PASS: all tests pass\n"
        "- Final grep: no source files referencing the pattern were missed\n"
    ),
    "thinking_v7": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## RULE #1: EDIT SOURCE FILES, NOT TEST FILES\n"
        "\n"
        "The Edit tool will BLOCK edits to test files. This is by design.\n"
        "The test tells you WHAT behavior is expected. SOURCE files are WHERE you fix it.\n"
        "\n"
        "How to identify SOURCE vs TEST files:\n"
        "- SOURCE: files in package directories (e.g., django/, sklearn/, xarray/, astropy/)\n"
        "- TEST: files in tests/, test_*.py, *_test.py, conftest.py\n"
        "- When in doubt: call SourceDirs tool to list source directories, or ClassifyFiles on your grep results\n"
        "\n"
        "## THREE-STEP METHOD\n"
        "\n"
        "### STEP 1: SCOPE DISCOVERY (Turns 1-3, NO EDITS ALLOWED)\n"
        "\n"
        "1. Call SourceDirs to learn which directories contain source code\n"
        "2. Read the test_patch — understand what the test ASSERTS\n"
        "3. Read the 'Relevant Source Files' listed above\n"
        "4. Grep for the core pattern in SOURCE directories:\n"
        "   Bash: grep -rn 'PATTERN' --include='*.py' <source_dir>/ | grep -v __pycache__\n"
        "   Note: grep ONLY in source directories (not tests/)\n"
        "5. Call ClassifyFiles on the grep results to confirm they are source files\n"
        "6. COUNT the results. Write TodoWrite checklist with EVERY source file.\n"
        "\n"
        "SCOPE DETECTION:\n"
        "- 1-3 files = NARROW → focused edits\n"
        "- 4-9 files = MEDIUM → read each file, edit carefully\n"
        "- 10+ files = BROAD REFACTOR → same transformation applied to many files\n"
        "\n"
        "For BROAD refactors:\n"
        "- Read 2-3 files to understand the pattern, then BatchEdit the rest\n"
        "- Use BatchEdit to apply the same replacement to ALL source files in ONE call\n"
        "- Target: complete all edits in 2-3 BatchEdit calls\n"
        "\n"
        "### STEP 2: EDIT SOURCE FILES (Turns 4-20)\n"
        "\n"
        "1. Edit ONLY files in source directories (confirmed by SourceDirs/ClassifyFiles)\n"
        "2. The Edit tool blocks test files — if blocked, grep for the source file instead\n"
        "3. Mark each file 'completed' in TodoWrite after editing\n"
        "4. For BROAD refactors: use BatchEdit (5-10 files per call)\n"
        "\n"
        "### STEP 3: VERIFY (Turns 20-30)\n"
        "\n"
        "1. Run FAIL_TO_PASS tests\n"
        "2. If tests fail:\n"
        "   a. Read the ACTUAL error output (not the test code)\n"
        "   b. Trace: test calls X → X imports Y → Y is defined in file Z → edit Z\n"
        "   c. Grep again — did you miss source files?\n"
        "   d. Fix and re-run\n"
        "\n"
        "## ANTI-PATTERNS (these cause 95% of failures)\n"
        "- Editing test files instead of source files (BLOCKED by tool — but don't waste turns trying)\n"
        "- Editing only 2-3 files when 15+ need changes (grep count > edit count = failure)\n"
        "- Fixing error messages instead of root causes\n"
        "- Re-reading test code instead of reading test ERROR output\n"
        "- Editing __init__.py re-exports before editing the actual implementation\n"
        "\n"
        "## GREP STRATEGIES FOR FILE DISCOVERY\n"
        "- Class name → also grep for the module it's imported from\n"
        "- Function name → also grep for callers and importers\n"
        "- Error message → grep for where it's raised\n"
        "- Import path → grep for both 'from X import' and 'import X'\n"
        "- ALWAYS grep in source directories only (not tests/)\n"
        "\n"
        "## BEFORE SAYING 'DONE'\n"
        "- TodoWrite: ALL items completed\n"
        "- FAIL_TO_PASS: all tests pass\n"
        "- Final grep: no source files referencing the pattern were missed\n"
    ),
    "thinking_v8": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## CORE PRINCIPLE: FIX ROOT CAUSES, NOT SYMPTOMS\n"
        "\n"
        "The test REPORTS the problem. The source code CAUSES the problem.\n"
        "You must always fix the CAUSE — never modify the file that reports the error.\n"
        "If you find yourself editing the test file, you have not traced far enough.\n"
        "Go back and find the source code that produces the wrong behavior.\n"
        "\n"
        "## REASONING METHOD: TRACE → DIAGNOSE → FIX → VERIFY\n"
        "\n"
        "Every task follows four phases. Complete each before moving to the next.\n"
        "Going backward to re-diagnose is expected and correct — it means you learned something.\n"
        "\n"
        "### PHASE 1: UNDERSTAND EXPECTED BEHAVIOR (turns 1-2)\n"
        "\n"
        "Goal: Form a precise statement of what the code SHOULD do.\n"
        "\n"
        "1. Read the failing test or error output. What does the test ASSERT?\n"
        "2. State in one sentence: 'Expected: X. Actual: Y.'\n"
        "3. Identify the core symbol: function name, class, or error message\n"
        "4. If test code is provided, note what it imports and calls\n"
        "\n"
        "Checkpoint: Can you state the expected behavior in one sentence?\n"
        "If not, re-read the test or error output before proceeding.\n"
        "\n"
        "### PHASE 2: CAUSAL TRACING (turns 2-5)\n"
        "\n"
        "Goal: Trace from the test assertion to the exact SOURCE code that causes the failure.\n"
        "This is the most important phase. Do not skip or rush it.\n"
        "You MUST find the source file before editing anything.\n"
        "\n"
        "**Step-by-step backward trace:**\n"
        "1. The test calls function F — what file DEFINES F? (not where F is imported, where it's defined)\n"
        "2. Follow the import chain: 'from A.B import F' → open A/B.py → find 'def F' or 'class F'\n"
        "3. If F delegates to helper H — find where H is defined\n"
        "4. If F is a method on class C — find where C is defined (may be different file from where it's imported)\n"
        "5. The DEFINITION file is where you edit. The file that imports it is not.\n"
        "\n"
        "**When tracing is difficult:**\n"
        "- If grep finds only test files: broaden the pattern, try the class name instead of method\n"
        "- If the symbol is re-exported via __init__.py: grep for 'def SYMBOL' or 'class SYMBOL' directly\n"
        "- If the package has subdirectories: grep recursively with --include='*.py'\n"
        "- Always exclude test directories from grep: grep -rn 'PATTERN' --include='*.py' . | grep -v test | grep -v __pycache__\n"
        "\n"
        "**Form explicit hypotheses:**\n"
        "- H1: [specific claim about what's wrong and in which source file] — confirmed/denied by [evidence]\n"
        "- H2: [alternative claim] — confirmed/denied by [evidence]\n"
        "\n"
        "**Gather evidence:**\n"
        "1. Read source files in the call chain (not test files)\n"
        "2. Grep for the core symbol across the entire codebase\n"
        "3. COUNT how many source files contain the symbol\n"
        "\n"
        "**Scope assessment:**\n"
        "- 1-3 files → NARROW: focused fix\n"
        "- 4-9 files → MEDIUM: read each, edit carefully\n"
        "- 10+ files → BROAD: identify the common transformation, apply systematically\n"
        "\n"
        "Checkpoint: Do you know which SOURCE files cause the failure? If not, keep tracing.\n"
        "Do NOT proceed to Phase 3 until you have identified at least one source file to edit.\n"
        "\n"
        "### PHASE 3: IMPLEMENT FIX (turns 5-25)\n"
        "\n"
        "Goal: Edit the SOURCE code that causes the failure.\n"
        "\n"
        "**MANDATORY PRE-EDIT CHECK — ask before EVERY edit:**\n"
        "1. Does this file CAUSE the problem, or does it REPORT the problem?\n"
        "2. Did I trace from the test to this specific file through the import/call chain?\n"
        "3. Is this file in a source directory (not tests/, test_*, conftest.py)?\n"
        "If ANY answer is unclear: stop and grep for the symbol's definition first.\n"
        "\n"
        "**Rules:**\n"
        "1. NEVER modify the file that reports the error — always fix what causes it\n"
        "2. For BROAD changes (10+ files):\n"
        "   - Read 2-3 files to confirm the transformation pattern\n"
        "   - Apply the SAME transformation to ALL affected source files\n"
        "   - Do NOT stop after 2-3 edits when grep found 15+ files\n"
        "   - Check __init__.py re-exports if you renamed or moved something\n"
        "3. After each batch of edits:\n"
        "   - Re-grep: are there source files you missed?\n"
        "   - Check imports: did you update all callers?\n"
        "\n"
        "### PHASE 4: VERIFY WITH EVIDENCE (turns 25-35)\n"
        "\n"
        "Goal: Confirm the fix works. No assumptions — only test output counts.\n"
        "\n"
        "1. Run the failing test(s)\n"
        "2. If PASS: re-grep to confirm you didn't miss any source files\n"
        "3. If FAIL:\n"
        "   a. Read the ACTUAL error output — not the test code\n"
        "   b. Re-trace: error mentions module A → where is A DEFINED → what does A do wrong?\n"
        "   c. Update your hypothesis (return to Phase 2)\n"
        "   d. Fix the source and re-run\n"
        "\n"
        "## COMMON FAILURE MODES\n"
        "\n"
        "- **Fixing symptoms**: Editing the file that reports the error instead of tracing to the source that causes it\n"
        "- **Scope underestimation**: Fixing 2-3 files when 15+ need changes\n"
        "- **Surface fixing**: Changing error messages instead of root causes\n"
        "- **Verification skipping**: Declaring done without running tests\n"
        "- **Intuition editing**: Guessing where code lives instead of tracing the import chain to its definition\n"
        "- **Re-reading tests**: Reading test code instead of test ERROR output\n"
        "\n"
        "## GREP STRATEGIES FOR DISCOVERY\n"
        "\n"
        "- Class name → grep for 'class ClassName' to find the DEFINITION, not usages\n"
        "- Function name → grep for 'def function_name' to find where it's defined\n"
        "- Error message → grep for where it's raised\n"
        "- Import path → grep for both 'from X import' and 'import X'\n"
        "- After renaming → grep for old name to find missed references\n"
        "- Always exclude test dirs: grep -rn 'PATTERN' --include='*.py' . | grep -v test | grep -v __pycache__\n"
    ),
    "thinking_v8.2": (
        "You are RustyCode, an AI coding assistant.\n"
        "Output complete working code. No placeholders, no TODOs.\n"
        "\n"
        "## CORE PRINCIPLE: FIX ROOT CAUSES, NOT SYMPTOMS\n"
        "\n"
        "The test REPORTS the problem. The source code CAUSES the problem.\n"
        "You must always fix the CAUSE — never modify the file that reports the error.\n"
        "If you find yourself editing the test file, you have not traced far enough.\n"
        "Go back and find the source code that produces the wrong behavior.\n"
        "\n"
        "## REASONING METHOD: TRACE → DIAGNOSE → PLAN → FIX → VERIFY\n"
        "\n"
        "Every task follows five phases. Complete each before moving to the next.\n"
        "Going backward is expected and correct — it means you learned something.\n"
        "\n"
        "### PHASE 1: UNDERSTAND EXPECTED BEHAVIOR (turns 1-2)\n"
        "\n"
        "Goal: Form a precise statement of what the code SHOULD do.\n"
        "\n"
        "1. Read the failing test or error output. What does the test ASSERT?\n"
        "2. State in one sentence: 'Expected: X. Actual: Y.'\n"
        "3. Identify the core symbol: function name, class, or error message\n"
        "4. If test code is provided, note what it imports and calls\n"
        "\n"
        "### PHASE 2: CAUSAL TRACING (turns 2-6)\n"
        "\n"
        "Goal: Trace from the test to the exact SOURCE code that causes the failure.\n"
        "You MUST find the source file before editing anything.\n"
        "\n"
        "**Step-by-step backward trace:**\n"
        "1. Test calls function F — what file DEFINES F? (not where it's imported, where it's DEFINED)\n"
        "2. Follow imports: 'from A.B import F' → open A/B.py → find 'def F' or 'class F'\n"
        "3. If F delegates to G — find where G is defined (may be in a different module)\n"
        "4. If F is a method on class C — find C's definition file\n"
        "5. Keep tracing deeper if needed: F calls G, G calls H — find where the ACTUAL logic lives\n"
        "\n"
        "**Deep tracing for complex frameworks:**\n"
        "When the bug involves a framework (Django, pytest, etc.):\n"
        "- The test may call a high-level API that delegates through 3-5 layers\n"
        "- Trace each layer: API method → manager → queryset → compiler → executor\n"
        "- Read each layer to understand what it does and where the behavior diverges\n"
        "- The bug is usually in the layer where behavior changes from expected to actual\n"
        "- If grep in the top-level module doesn't find the issue, grep in subdirectories\n"
        "\n"
        "**When stuck:**\n"
        "- Broaden the pattern: try the class name instead of method name\n"
        "- Grep for the error message text to find where it's raised\n"
        "- Search subdirectories: grep -rn 'PATTERN' --include='*.py' subdir/\n"
        "- Exclude test dirs: | grep -v test | grep -v __pycache__\n"
        "\n"
        "**Form explicit hypotheses:**\n"
        "- H1: [what's wrong and in which source file] — evidence: [what confirms/denies]\n"
        "- H2: [alternative] — evidence: [what confirms/denies]\n"
        "\n"
        "**Scope assessment:**\n"
        "- 1-3 files → NARROW | 4-9 → MEDIUM | 10+ → BROAD\n"
        "\n"
        "Gate: Do NOT proceed until you have identified at least one SOURCE file to edit.\n"
        "\n"
        "### PHASE 3: FIX PLAN (turns 4-8)\n"
        "\n"
        "Goal: Write down the COMPLETE fix before touching any code.\n"
        "This prevents partial fixes and missed changes.\n"
        "\n"
        "Use TodoWrite to create a checklist with EVERY change needed:\n"
        "1. For each source file: what specific lines change and why\n"
        "2. For each new function/method: what it does and where it's added\n"
        "3. For each caller that needs updating: which file, which line, what changes\n"
        "4. For __init__.py: any re-exports to add/update\n"
        "\n"
        "**Completeness checks before proceeding:**\n"
        "- Did you grep for ALL references to the changed symbol?\n"
        "- If adding a new method: does anything need to CALL it?\n"
        "- If changing behavior: do callers need to be updated?\n"
        "- If renaming: did you find ALL importers and re-exporters?\n"
        "\n"
        "Gate: TodoWrite checklist must be COMPLETE before any edits.\n"
        "\n"
        "### PHASE 4: IMPLEMENT FIX (turns 8-25)\n"
        "\n"
        "Goal: Make the edits from your Fix Plan.\n"
        "\n"
        "**Pre-edit check for EACH edit:**\n"
        "1. Does this file CAUSE or REPORT the problem?\n"
        "2. Is this file in a source directory?\n"
        "3. Is this edit in my Fix Plan checklist?\n"
        "\n"
        "**Rules:**\n"
        "1. NEVER edit the file that reports the error\n"
        "2. Follow the Fix Plan checklist — mark each item done\n"
        "3. For BROAD changes: apply the same transformation to ALL files\n"
        "4. After edits: re-grep to verify no source files were missed\n"
        "\n"
        "### PHASE 5: VERIFY WITH EVIDENCE (turns 25-35)\n"
        "\n"
        "Goal: Confirm the fix works. Only test output counts.\n"
        "\n"
        "1. Run the failing test(s)\n"
        "2. If PASS: verify TodoWrite checklist is fully complete, re-grep for missed files\n"
        "3. If FAIL:\n"
        "   a. Read the ACTUAL error output — not the test code\n"
        "   b. Is the error about a file you SHOULD have edited but didn't?\n"
        "   c. Update your Fix Plan, return to Phase 4\n"
        "\n"
        "## COMMON FAILURE MODES\n"
        "\n"
        "- **Fixing symptoms**: Editing the file that reports the error instead of the source that causes it\n"
        "- **Partial fixes**: Editing one file when the fix requires changes in 2-3 files\n"
        "- **Missing callers**: Adding a new method but not updating the code that should call it\n"
        "- **Scope underestimation**: Fixing 2-3 files when 15+ need changes\n"
        "- **Shallow tracing**: Stopping at the first layer instead of tracing deeper into the framework\n"
        "- **Intuition editing**: Guessing where code lives instead of tracing the definition chain\n"
        "\n"
        "## GREP STRATEGIES\n"
        "\n"
        "- Find definitions: grep -rn 'def FUNCTION' or 'class CLASS' --include='*.py'\n"
        "- Find all references: grep -rn 'SYMBOL' --include='*.py' | grep -v test\n"
        "- Find error sources: grep -rn 'raise.*ERROR_TEXT' --include='*.py'\n"
        "- Find imports: grep -rn 'from X import\\|import X' --include='*.py'\n"
        "- Deep search subdirectories: grep -rn 'PATTERN' --include='*.py' db/ models/ orm/\n"
    ),
    "thinking_v9": (
        "You are an expert software engineer. Fix bugs by understanding the actual error, "
        "tracing to the source, and making precise edits.\n"
        "\n"
        "## APPROACH\n"
        "\n"
        "1. **Read the error** — the test output tells you exactly what's wrong. Study it.\n"
        "2. **Find the source** — trace from the error to the file that CAUSES it (not the file that reports it).\n"
        "   - Test imports `from A.B import F` → open `A/B.py` → find `def F` or `class F`\n"
        "   - If F calls G → find where G is defined. Keep tracing until you reach the root cause.\n"
        "   - Use: `grep -rn 'def FUNCTION' --include='*.py' . | grep -v test | grep -v __pycache__`\n"
        "3. **Read the source** — understand what the code does now vs what it should do.\n"
        "4. **Edit the source** — make the minimal change that fixes the behavior.\n"
        "5. **Run the test** — verify. If it fails, read the NEW error output and adjust.\n"
        "\n"
        "## RULES\n"
        "\n"
        "- NEVER edit test files. The test reports the bug; source code causes it.\n"
        "- NEVER skip running the test. You must verify your fix.\n"
        "- If the fix requires changes in multiple files, use `grep -rn 'SYMBOL' --include='*.py' . | grep -v test` "
        "to find ALL files that reference the symbol, then edit each one.\n"
        "- If your first fix doesn't work, read the test error carefully and try a different approach.\n"
        "- For framework bugs (Django, pytest, etc.): the high-level API may delegate through many layers. "
        "Trace each layer until you find where behavior diverges from expected.\n"
        "\n"
        "## TOOLS\n"
        "\n"
        "- Use `Bash` for: running tests, grep, find, git operations\n"
        "- Use `Read` for: reading source files (NOT test files)\n"
        "- Use `Edit` for: making precise changes to source files\n"
        "- Use `Grep` for: finding where symbols are defined or used\n"
        "- Use `TodoWrite` for: tracking a multi-file fix plan before editing\n"
        "- Use `BatchEdit` for: applying the same change across many files\n"
        "\n"
        "## EFFICIENCY\n"
        "\n"
        "- Make MULTIPLE tool calls per turn when possible (read several files, grep with multiple patterns).\n"
        "- Don't re-read files you've already seen.\n"
        "- Run the failing test as soon as you've made edits — don't wait.\n"
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
            "CRITICAL: Refactors typically touch MANY files (10-30+). Do NOT stop after 2-3 edits.\n"
            "Strategy:\n"
            "1. Identify the OLD pattern from the issue description (what's being renamed/replaced)\n"
            "2. grep -rn 'OLD_PATTERN' --include='*.py' . | grep -v test → COUNT the results\n"
            "3. Also grep for related patterns: imports, callers, re-exports in __init__.py\n"
            "4. Use BatchEdit to apply the same replacement across all affected files at once\n"
            "5. After editing, grep AGAIN to verify no references were missed\n"
            "6. Run tests to confirm behavior preserved\n"
        )
    return (
        "\n## Instance Type: BUG FIX\n"
        "The test IS the specification. Trace the actual failure, not the error message.\n"
    )


# ── Turn-Based Nudges ──────────────────────────────────────────────

NUDGES = {
    2: (
        "NUDGE: Read the test_patch code NOW. It defines what correct behavior looks like. "
        "Then grep for the core pattern to count how many source files need changes."
    ),
    4: (
        "NUDGE: SCOPE CHECK — grep for the key symbol/pattern across ALL source files (exclude tests). "
        "COUNT the results. If 5+ source files contain the pattern, this is a BROAD change. "
        "Write a TodoWrite checklist with EVERY file before editing anything."
    ),
    6: (
        "NUDGE: You should be editing source files by now. "
        "If your TodoWrite checklist has 10+ files, use BatchEdit to apply the same change to multiple files at once. "
        "BatchEdit is much faster than editing files one by one. "
        "If you haven't grepped yet, STOP and grep for the pattern first."
    ),
    8: (
        "NUDGE: BREADTH CHECK — grep for the pattern again. "
        "How many source files did you edit vs how many contain the pattern? "
        "If edited_count < grep_count, you have more files to update. Do NOT skip files."
    ),
    10: (
        "NUDGE: Are you editing test files? STOP. Edit SOURCE files only. "
        "The test tells you WHAT to fix. Source files are WHERE to fix. "
        "Check your TodoWrite — how many items still pending?"
    ),
    12: (
        "NUDGE: If tests failed, re-read the ERROR OUTPUT (not the test code). "
        "Trace: what function does the test call → what does it import → where is that defined? "
        "The error may name module A but the real fix is in module B."
    ),
    15: (
        "NUDGE: PROGRESS CHECK — How many checklist items completed? "
        "If less than half, batch your edits faster. "
        "For broad refactors: apply the same transformation to every file — don't overthink each one."
    ),
    18: (
        "NUDGE: Try a different approach. If you've been editing one file, look at others. "
        "Grep with a BROADER pattern — maybe you missed files with slightly different imports. "
        "Re-read the test error from scratch."
    ),
    25: (
        "NUDGE: Final attempt. Make your simplest possible fix — one line change, one import fix, "
        "one parameter addition. What's the most obvious thing that could fix the test?"
    ),
}

NUDGES_V8 = {
    2: (
        "PHASE 1 CHECK: Can you state the expected behavior in one sentence? "
        "If not, re-read the test/error output. What does the test ASSERT?"
    ),
    4: (
        "PHASE 2 CHECK: You must find the SOURCE file that CAUSES the failure. "
        "Trace: test calls F → grep for 'def F' or 'class F' to find where F is DEFINED. "
        "The definition file is where you edit. COUNT how many source files match."
    ),
    6: (
        "PRE-EDIT CHECK: Before editing, ask: Does this file CAUSE the problem "
        "or REPORT the problem? If it's a test file, STOP — you haven't traced far enough. "
        "Grep for the symbol's DEFINITION in source directories."
    ),
    8: (
        "ROOT CAUSE CHECK: Are you fixing what CAUSES the error, or what REPORTS it? "
        "The test file REPORTS the problem. The source file CAUSES it. "
        "You must edit the file that CAUSES the wrong behavior."
    ),
    10: (
        "EVIDENCE CHECK: If tests failed, re-read the ERROR OUTPUT (not the test code). "
        "The error names module A → where is A DEFINED (not imported, DEFINED)? "
        "Find the definition and fix it there."
    ),
    15: (
        "BREADTH CHECK: Grep for 'def SYMBOL' or 'class SYMBOL' again with broader patterns. "
        "Did you miss source files with slightly different names or in subdirectories? "
        "For broad changes: same transformation everywhere, don't customize per file."
    ),
    20: (
        "PHASE 4 CHECK: Run the actual test. Read the error carefully. "
        "If still failing, return to Phase 2 — your hypothesis may be wrong. "
        "Re-trace from the error to the source DEFINITION."
    ),
    25: (
        "Final attempt: What is the SIMPLEST possible fix to the SOURCE code? "
        "One line change, one import fix, one parameter addition. "
        "Re-read the original error from scratch."
    ),
}

# Versioned nudge lookup
NUDGES_BY_VERSION = {
    "thinking_v8": NUDGES_V8,
    "thinking_v8.2": NUDGES_V8,
    "thinking_v9": {
        3: (
            "CHECKPOINT: Have you found the SOURCE file that causes the failure? "
            "If you've only read test files, STOP. grep for 'def FUNCTION' or 'class CLASS' "
            "to find where the code is DEFINED, not where it's used."
        ),
        8: (
            "VERIFY: Run the failing test NOW. Don't keep editing without testing. "
            "Read the actual error output — it tells you exactly what's wrong."
        ),
        15: (
            "SCOPE CHECK: grep for the symbol again. How many source files contain it? "
            "If you edited 2 files but grep finds 10, you have more work to do. "
            "Use BatchEdit for the same change across multiple files."
        ),
        22: (
            "RE-READ THE ERROR: If the test still fails, read the error output carefully. "
            "It may name a DIFFERENT file or function than what you've been editing. "
            "Trace from the error message to the source definition."
        ),
        30: (
            "LAST ATTEMPT: What is the simplest possible fix? One line change, one import, "
            "one parameter addition. Re-read the original error from scratch."
        ),
    },
}


# ── Import Map (auto file discovery from test_patch) ──────────────────

def build_import_map(test_patch: str, repo_dir) -> str:
    """Extract import paths, symbols, and test files from test_patch.

    Provides the model with immediate file discovery without spending turns.
    Returns:
    - Source files resolved from imports
    - Source files found by grepping for symbols referenced in the test
    - Test file paths from the diff (for reference)
    - Suggested grep patterns for broad discovery
    """
    import re

    sections = []

    # 1. Extract test file paths from diff headers
    test_files = []
    for line in test_patch.splitlines():
        m = re.match(r'^\+\+\+ b/(.+\.py)', line)
        if m:
            test_files.append(m.group(1))

    # 2. Extract import lines from the test_patch
    imports = set()
    symbols = set()
    for line in test_patch.splitlines():
        if line.startswith(("---", "+++", "@@", "-")):
            continue
        stripped = line.lstrip("+").strip()

        # Match "from X import Y, Z" and "import X"
        m = re.match(r'(?:from\s+(\S+)\s+import\s+(.+?)$|import\s+(.+?)(?:\s+as|\s*,|$))', stripped)
        if m:
            mod = (m.group(1) or m.group(3) or "").strip().rstrip(",")
            if mod and not mod.startswith("."):
                imports.add(mod)
            # Extract imported symbols
            if m.group(2):
                for sym in m.group(2).split(","):
                    sym = sym.strip().split(" as ")[0].strip()
                    if sym and sym != "*":
                        symbols.add(sym)

    # 3. Extract dotted symbol references (e.g., mtri.triinterpolate._cg)
    for line in test_patch.splitlines():
        if line.startswith(("---", "+++", "@@", "-")):
            continue
        for m in re.finditer(r'(?<![a-zA-Z])([a-z_][a-z0-9_]*(?:\.[a-z_][a-z0-9_]*){2,})', line.lstrip("+")):
            ref = m.group(1)
            # Skip common false positives (sys.path, os.environ, etc.)
            prefix = ref.split(".")[0]
            if prefix not in ("sys", "os", "re", "json", "math", "copy", "self", "true", "false", "none"):
                symbols.add(ref.split(".")[-1])

    # 4. Resolve imports to source files
    resolved = []
    for mod in sorted(imports)[:15]:
        parts = mod.split(".")
        patterns = [
            "/".join(parts) + ".py",
            "/".join(parts[:-1]) + "/__init__.py" if len(parts) > 1 else None,
        ]
        for pattern in patterns:
            if pattern is None:
                continue
            result = subprocess.run(
                ["find", str(repo_dir), "-path", f"*/{pattern}", "-not", "-path", "*/.*",
                 "-not", "-path", "*/__pycache__/*", "-not", "-path", "*/test*"],
                capture_output=True, text=True, timeout=10,
            )
            for match in result.stdout.strip().splitlines()[:3]:
                rel = os.path.relpath(match, str(repo_dir))
                resolved.append(f"  `{mod}` → {rel}")

    # 5. Resolve symbols via grep (top symbols only)
    symbol_resolved = []
    for sym in sorted(symbols)[:8]:
        if len(sym) < 3:
            continue
        result = subprocess.run(
            ["grep", "-rn", f"\\b{sym}\\b", "--include=*.py", "-l", str(repo_dir)],
            capture_output=True, text=True, timeout=15,
        )
        source_files = []
        for match in result.stdout.strip().splitlines():
            rel = os.path.relpath(match.strip(), str(repo_dir))
            # Skip test files, cache, hidden dirs
            if "/test" in rel or "/__pycache__" in rel or "/." in rel:
                continue
            source_files.append(rel)
        if source_files:
            file_count = len(source_files)
            files_str = ", ".join(source_files[:6])
            if file_count > 6:
                files_str += f", ... ({file_count} total)"
            symbol_resolved.append(f"  `{sym}` found in: {files_str}")

    # 6. Compute total unique source files found
    all_source_files = set()
    for line_list in [resolved, symbol_resolved]:
        for line in line_list:
            # Extract file paths from output lines
            for part in line.split():
                if part.endswith(".py") and "/" in part and "/test" not in part:
                    all_source_files.add(part.rstrip(","))

    # 7. Build output sections
    if resolved:
        sections.append("## Source Files (from test imports)\n\nThe test imports these modules — read them first:\n" + "\n".join(resolved))

    if symbol_resolved:
        sections.append("## Source Files (from symbol grep)\n\nSymbols referenced by the test — these files likely need changes:\n" + "\n".join(symbol_resolved))

    # Scope hint based on total files found
    total_found = len(all_source_files)
    if total_found >= 10:
        sections.append(
            f"## SCOPE WARNING: BROAD CHANGE ({total_found}+ source files detected)\n\n"
            "This is a BROAD refactor. You MUST edit ALL files listed above.\n"
            "Strategy: read 2-3 files to understand the pattern, then batch-edit the rest.\n"
            "Target: edit 5-8 files per turn. Do NOT stop after editing only 3-4 files."
        )
    elif total_found >= 5:
        sections.append(
            f"## Scope Note: {total_found}+ source files detected\n\n"
            "This change affects multiple files. Ensure you edit ALL of them."
        )

    if test_files:
        sections.append("## Test Files (for reference)\n\n" + "\n".join(f"  {f}" for f in test_files[:5]))

    # 8. Suggest grep patterns based on extracted symbols
    grep_suggestions = []
    for sym in sorted(symbols)[:5]:
        if len(sym) >= 3:
            grep_suggestions.append(f"  grep -rn '{sym}' --include='*.py' . | grep -v test | grep -v __pycache__")
    if grep_suggestions:
        sections.append("## Recommended Grep Commands\n\nUse these to find ALL affected files:\n" + "\n".join(grep_suggestions))

    return ("\n" + "\n\n".join(sections) + "\n") if sections else ""


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

    # Detect source directories and add to prompt (v7)
    source_dirs_section = ""
    source_dirs = detect_source_dirs(repo_dir)
    if source_dirs:
        source_dirs_section = (
            f"\n## SOURCE DIRECTORIES (edit ONLY these)\n"
            f"Source code directories: {', '.join(source_dirs)}\n"
            f"DO NOT edit files outside these directories (especially NOT tests/ or test_*.py).\n"
            f"Your grep commands should target these directories: grep -rn 'PATTERN' {' '.join(source_dirs)}\n"
        )

    hints_section = ""
    if args.hints and inst.get("hints_text"):
        hints_section = f"\n## Developer Hints\n{inst['hints_text'][:2000]}\n"

    user_prompt = f"""Please fix the following issue in this repository.

## Repository Structure

```
{file_tree}
```
{source_dirs_section}{import_map}{type_guidance}
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
    tool_counts = {"Bash": 0, "Read": 0, "Write": 0, "Edit": 0, "Grep": 0, "Glob": 0, "ListDir": 0, "FindReferences": 0, "GetSymbols": 0, "GitDiff": 0, "TodoWrite": 0, "BatchEdit": 0, "ClassifyFiles": 0, "SourceDirs": 0}

    print(f"\n{'='*60}")
    print(f"  {inst['instance_id']}")
    print(f"  Model: {args.model}  Max turns: {max_turns}  Prompt: {args.system_prompt}")
    print(f"{'='*60}")

    start = time.time()

    for turn in range(max_turns):
        print(f"\n--- Turn {turn + 1}/{max_turns} ---")

        try:
            # Use streaming for Opus (required for requests > 10 min)
            with client.messages.stream(
                model=args.model,
                max_tokens=args.max_tokens,
                system=SYSTEM_PROMPTS[args.system_prompt],
                tools=TOOLS,
                messages=messages,
            ) as stream:
                response = stream.get_final_message()
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

            result_text = run_tool(name, input_args, repo_dir, args.verbose, args.system_prompt)
            is_error = result_text.startswith("ERROR:")

            if is_error:
                total_errors += 1
            if name in ("Write", "Edit", "BatchEdit"):
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
        active_nudges = NUDGES_BY_VERSION.get(args.system_prompt, NUDGES)
        if next_turn in active_nudges:
            nudge_text = active_nudges[next_turn]
            messages.append({"role": "user", "content": [{"type": "text", "text": nudge_text}]})
            messages.append({"role": "assistant", "content": "Understood. I will adjust my approach."})
            if args.verbose:
                print(f"  NUDGE @ turn {next_turn}: {nudge_text[:80]}...")

        # v7: Post-turn test-file detection and REVERSION via git diff
        if args.system_prompt == "thinking_v7" and edits_made > 0 and turn >= 3:
            try:
                diff_result = subprocess.run(
                    ["git", "diff", "--name-only"],
                    cwd=repo_dir, capture_output=True, text=True, timeout=10,
                )
                modified = diff_result.stdout.strip().splitlines()
                test_mods = [f for f in modified if is_test_file(f)]
                if test_mods:
                    # REVERT test file changes
                    subprocess.run(
                        ["git", "checkout", "--"] + test_mods,
                        cwd=repo_dir, capture_output=True, timeout=10,
                    )
                    source_dirs = detect_source_dirs(repo_dir)
                    source_hint = ", ".join(source_dirs[:5]) if source_dirs else "lib/, src/"
                    warning = (
                        f"CRITICAL: Your changes to {len(test_mods)} TEST file(s) have been REVERTED.\n"
                        f"Reverted: {', '.join(test_mods[:5])}\n"
                        f"You MUST edit SOURCE files in {source_hint}/ directories.\n"
                        f"Use: grep -rn 'PATTERN' {' '.join(source_dirs[:3])} to find source files to edit."
                    )
                    messages.append({"role": "user", "content": [{"type": "text", "text": warning}]})
                    messages.append({"role": "assistant", "content": "Understood. I will grep in source directories and edit only source files."})
                    if args.verbose:
                        print(f"  REVERTED {len(test_mods)} test file changes: {', '.join(test_mods[:3])}")
            except Exception:
                pass

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
    parser.add_argument("--model", default="claude-opus-4-7", help="Model to use")
    parser.add_argument("--max-turns", type=int, default=40, help="Max agent turns")
    parser.add_argument("--max-tokens", type=int, default=32768, help="Max output tokens")
    parser.add_argument("--pretest", action="store_true", default=True, help="Pre-run FAIL_TO_PASS tests and include output in prompt (default: True)")
    parser.add_argument("--no-pretest", dest="pretest", action="store_false", help="Disable pretest")
    parser.add_argument("--hints", action="store_true", default=True, help="Include hints_text in prompt (default: True)")
    parser.add_argument("--no-hints", dest="hints", action="store_false", help="Disable hints")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show tool calls and output")
    parser.add_argument("--system-prompt", choices=["minimal", "structured", "agentic", "enhanced", "thinking", "thinking_v2", "thinking_v3", "thinking_v4", "thinking_v5", "thinking_v6", "thinking_v7", "thinking_v8", "thinking_v8.2", "thinking_v9"], default="thinking_v9",
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

        # Incremental save after each instance
        if args.output:
            with open(args.output, "w") as f:
                json.dump(predictions, f, indent=2)

    # Save results
    if args.output:
        print(f"\nSaved to {args.output}")

    print(f"\n{'='*60}")
    print(f"  TOTAL: {len(instances)}  PATCHES: {patches}  EMPTY: {len(instances) - patches}  RESOLVED: {resolved}")
    print(f"  Patch rate: {100*patches/len(instances):.0f}%  Resolve rate: {100*resolved/len(instances):.0f}%")
    print(f"{'='*60}")


if __name__ == "__main__":
    main()
