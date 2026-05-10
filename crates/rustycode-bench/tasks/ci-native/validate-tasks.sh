#!/bin/bash
# Validate all CI native benchmark tasks

set -e

TASKS_DIR="/Users/nat/dev/rustycode/crates/rustycode-bench/tasks/ci-native"
ERRORS=0

echo "Validating CI native benchmark tasks..."
echo ""

for task_dir in "$TASKS_DIR"/*; do
    if [ ! -d "$task_dir" ]; then
        continue
    fi

    task_name=$(basename "$task_dir")
    echo "Checking $task_name..."

    # Check required files
    if [ ! -f "$task_dir/task.toml" ]; then
        echo "  ✗ Missing task.toml"
        ERRORS=$((ERRORS + 1))
    fi

    if [ ! -f "$task_dir/instruction.md" ]; then
        echo "  ✗ Missing instruction.md"
        ERRORS=$((ERRORS + 1))
    fi

    if [ ! -f "$task_dir/tests/verify.sh" ]; then
        echo "  ✗ Missing tests/verify.sh"
        ERRORS=$((ERRORS + 1))
    else
        # Check if verify.sh is executable
        if [ ! -x "$task_dir/tests/verify.sh" ]; then
            echo "  ✗ verify.sh is not executable"
            ERRORS=$((ERRORS + 1))
        fi
    fi

    # Parse task.toml to get metadata
    if [ -f "$task_dir/task.toml" ]; then
        difficulty=$(grep -E '^difficulty = ' "$task_dir/task.toml" | head -1 | cut -d'"' -f2 || echo "unknown")
        category=$(grep -E '^category = ' "$task_dir/task.toml" | head -1 | cut -d'"' -f2 || echo "unknown")
        echo "  ✓ [$difficulty] $category"
    fi
done

echo ""
if [ $ERRORS -eq 0 ]; then
    echo "✓ All tasks validated successfully!"
    echo ""
    echo "Task Summary:"
    echo "  - File I/O: sort-csv, merge-json"
    echo "  - Code Generation: python-fibonacci, python-fizzbuzz"
    echo "  - Text Processing: extract-urls, word-count"
    echo "  - Scripting: file-organizer, backup-script"
    echo "  - Refactoring: extract-function, rename-variables"
    echo ""
    echo "Total: 10 tasks"
    exit 0
else
    echo "✗ Found $ERRORS validation errors"
    exit 1
fi
