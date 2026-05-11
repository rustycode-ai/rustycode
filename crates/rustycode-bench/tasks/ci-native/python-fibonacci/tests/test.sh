#!/bin/bash
set -e

# Check if fib.py exists
if [ ! -f "fib.py" ]; then
    echo "FAIL: fib.py not found"
    exit 1
fi

# Run the script and capture output
output=$(python3 fib.py 2>&1)
exit_code=$?

if [ $exit_code -ne 0 ]; then
    echo "FAIL: fib.py exited with error code $exit_code"
    echo "$output"
    exit 1
fi

# Check if we have exactly 20 lines
line_count=$(echo "$output" | wc -l)
if [ "$line_count" -ne 20 ]; then
    echo "FAIL: Expected 20 lines, got $line_count"
    exit 1
fi

# Check first few values
first_line=$(echo "$output" | sed -n '1p')
if [ "$first_line" != "0" ]; then
    echo "FAIL: Expected first line to be 0, got '$first_line'"
    exit 1
fi

second_line=$(echo "$output" | sed -n '2p')
if [ "$second_line" != "1" ]; then
    echo "FAIL: Expected second line to be 1, got '$second_line'"
    exit 1
fi

third_line=$(echo "$output" | sed -n '3p')
if [ "$third_line" != "1" ]; then
    echo "FAIL: Expected third line to be 1, got '$third_line'"
    exit 1
fi

# Check last value
last_line=$(echo "$output" | sed -n '20p')
if [ "$last_line" != "4181" ]; then
    echo "FAIL: Expected last line to be 4181, got '$last_line'"
    exit 1
fi

echo "PASS: Fibonacci sequence correct"
exit 0
