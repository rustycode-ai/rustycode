#!/bin/bash
set -e

# Check if fizzbuzz.py exists
if [ ! -f "fizzbuzz.py" ]; then
    echo "FAIL: fizzbuzz.py not found"
    exit 1
fi

# Run the script and capture output
output=$(python3 fizzbuzz.py 2>&1)
exit_code=$?

if [ $exit_code -ne 0 ]; then
    echo "FAIL: fizzbuzz.py exited with error code $exit_code"
    echo "$output"
    exit 1
fi

# Check if we have exactly 100 lines
line_count=$(echo "$output" | wc -l)
if [ "$line_count" -ne 100 ]; then
    echo "FAIL: Expected 100 lines, got $line_count"
    exit 1
fi

# Check specific cases
line_3=$(echo "$output" | sed -n '3p')
if [ "$line_3" != "Fizz" ]; then
    echo "FAIL: Line 3 should be 'Fizz', got '$line_3'"
    exit 1
fi

line_5=$(echo "$output" | sed -n '5p')
if [ "$line_5" != "Buzz" ]; then
    echo "FAIL: Line 5 should be 'Buzz', got '$line_5'"
    exit 1
fi

line_15=$(echo "$output" | sed -n '15p')
if [ "$line_15" != "FizzBuzz" ]; then
    echo "FAIL: Line 15 should be 'FizzBuzz', got '$line_15'"
    exit 1
fi

line_100=$(echo "$output" | sed -n '100p')
if [ "$line_100" != "Buzz" ]; then
    echo "FAIL: Line 100 should be 'Buzz', got '$line_100'"
    exit 1
fi

echo "PASS: FizzBuzz sequence correct"
exit 0
