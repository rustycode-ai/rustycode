#!/bin/bash
set -e

# Check if wordcount.txt exists
if [ ! -f "wordcount.txt" ]; then
    echo "FAIL: wordcount.txt not found"
    exit 1
fi

# Check if file has at most 10 lines
line_count=$(wc -l < wordcount.txt)
if [ "$line_count" -gt 10 ]; then
    echo "FAIL: Expected at most 10 lines, got $line_count"
    exit 1
fi

if [ "$line_count" -lt 1 ]; then
    echo "FAIL: wordcount.txt is empty"
    exit 1
fi

# Check format (each line should be "word: count")
while IFS= read -r line; do
    if [[ ! "$line" =~ ^[a-z]+:\ [0-9]+$ ]]; then
        echo "FAIL: Invalid format: $line"
        exit 1
    fi
done < wordcount.txt

# Check if counts are in descending order
prev_count=999999
while IFS=': ' read -r word count; do
    if [ "$count" -gt "$prev_count" ]; then
        echo "FAIL: Counts not in descending order (found $count after $prev_count)"
        exit 1
    fi
    prev_count=$count
done < wordcount.txt

# Check that all words are lowercase
if grep -E '[A-Z]' wordcount.txt >/dev/null 2>&1; then
    echo "FAIL: Words should be lowercase"
    exit 1
fi

echo "PASS: Word count format correct"
exit 0
