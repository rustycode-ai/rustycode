#!/bin/bash
set -e

# Check if process.py exists
if [ ! -f "process.py" ]; then
    echo "FAIL: process.py not found"
    exit 1
fi

# Check that single-letter variable names are gone (except in comments/strings)
# This is a heuristic check - we look for problematic patterns
if grep -E "^\s*[a-z]\s*=" process.py | grep -v "result\|final_price\|subtotal\|total" >/dev/null 2>&1; then
    echo "FAIL: Found single-letter variable assignments"
    exit 1
fi

# Check that descriptive names are present
if ! grep -q "quantity" process.py; then
    echo "FAIL: Variable 'quantity' not found"
    exit 1
fi

if ! grep -q "unit_price" process.py; then
    echo "FAIL: Variable 'unit_price' not found"
    exit 1
fi

if ! grep -q "tax" process.py; then
    echo "FAIL: Variable 'tax' not found"
    exit 1
fi

# Test that the code runs correctly
output=$(python3 process.py 2>&1)
exit_code=$?

if [ $exit_code -ne 0 ]; then
    echo "FAIL: process.py exited with error code $exit_code"
    echo "$output"
    exit 1
fi

# The output should be a number (8.0 based on the calculation)
if ! echo "$output" | grep -qE "^[0-9]+\.?[0-9]*$"; then
    echo "FAIL: Expected numeric output, got: $output"
    exit 1
fi

echo "PASS: Variables renamed correctly"
exit 0
