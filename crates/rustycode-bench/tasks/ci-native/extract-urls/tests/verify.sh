#!/bin/bash
set -e

# Check if urls.txt exists
if [ ! -f "urls.txt" ]; then
    echo "FAIL: urls.txt not found"
    exit 1
fi

# Check if file is not empty
if [ ! -s "urls.txt" ]; then
    echo "FAIL: urls.txt is empty"
    exit 1
fi

# Check if URLs are sorted (case-insensitive)
if ! sort -f -c urls.txt 2>/dev/null; then
    echo "FAIL: URLs are not sorted correctly"
    exit 1
fi

# Check if all lines contain http:// or https://
while IFS= read -r line; do
    if [[ ! "$line" =~ ^https?:// ]]; then
        echo "FAIL: Invalid URL format: $line"
        exit 1
    fi
done < urls.txt

# Check for expected URLs (case-insensitive)
if ! grep -iq "example.com" urls.txt; then
    echo "FAIL: Expected to find example.com"
    exit 1
fi

if ! grep -iq "github.com" urls.txt; then
    echo "FAIL: Expected to find github.com"
    exit 1
fi

echo "PASS: URLs extracted and sorted correctly"
exit 0
