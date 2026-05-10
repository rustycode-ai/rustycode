#!/bin/bash
set -e

# Check if calculate.py exists
if [ ! -f "calculate.py" ]; then
    echo "FAIL: calculate.py not found"
    exit 1
fi

# Check if the function exists
if ! grep -q "def calculate_circle_properties" calculate.py; then
    echo "FAIL: Function calculate_circle_properties not found"
    exit 1
fi

# Check if function returns a tuple
if ! grep -q "return area, circumference" calculate.py && ! grep -q "return (area, circumference)" calculate.py; then
    echo "FAIL: Function should return (area, circumference) tuple"
    exit 1
fi

# Check if main code uses the function
if ! grep -q "calculate_circle_properties(radius)" calculate.py; then
    echo "FAIL: Main code should call calculate_circle_properties(radius)"
    exit 1
fi

# Test that the code runs correctly (provide input)
output=$(echo "5.0" | python3 calculate.py 2>&1)
exit_code=$?

if [ $exit_code -ne 0 ]; then
    echo "FAIL: calculate.py exited with error code $exit_code"
    echo "$output"
    exit 1
fi

# Check if output contains expected values
if ! echo "$output" | grep -q "Area:"; then
    echo "FAIL: Output should contain 'Area:'"
    exit 1
fi

if ! echo "$output" | grep -q "Circumference:"; then
    echo "FAIL: Output should contain 'Circumference:'"
    exit 1
fi

echo "PASS: Function extracted correctly"
exit 0
