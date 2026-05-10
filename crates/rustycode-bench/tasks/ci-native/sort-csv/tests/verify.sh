#!/bin/bash
set -e

# Check if output file exists
if [ ! -f "output.csv" ]; then
    echo "FAIL: output.csv not found"
    exit 1
fi

# Normalize line endings (CRLF -> LF) for consistent processing
temp_file=$(mktemp)
tr -d '\r' < output.csv > "$temp_file"
mv "$temp_file" output.csv

# Check if output has correct number of lines (should be 5: header + 4 data rows)
line_count=$(wc -l < output.csv)
if [ "$line_count" -ne 5 ]; then
    echo "FAIL: Expected 5 lines, got $line_count"
    exit 1
fi

# Check if header is preserved
first_line=$(head -n 1 output.csv)
if [ "$first_line" != "name,age,score" ]; then
    echo "FAIL: Header not preserved correctly"
    exit 1
fi

# Check if data is sorted by age (extract age column, skip header)
ages=$(tail -n +2 output.csv | cut -d',' -f2)
prev_age=0
echo "$ages" | while read -r age; do
    if [ "$age" -lt "$prev_age" ]; then
        echo "FAIL: Data not sorted correctly by age"
        exit 1
    fi
    prev_age=$age
done

# Check specific expected order
second_line=$(sed -n '2p' output.csv)
if [ "$second_line" != "Bob,25,90" ]; then
    echo "FAIL: Expected 'Bob,25,90' as second line, got '$second_line'"
    exit 1
fi

echo "PASS: CSV sorted correctly"
exit 0
