#!/bin/bash
set -e

# Check if merged file exists
if [ ! -f "merged.json" ]; then
    echo "FAIL: merged.json not found"
    exit 1
fi

# Check if valid JSON
if ! python3 -c "import json; json.load(open('merged.json'))" 2>/dev/null; then
    echo "FAIL: merged.json is not valid JSON"
    exit 1
fi

# Check if it's an array
if ! python3 -c "import json; data = json.load(open('merged.json')); exit(0 if isinstance(data, list) else 1)" 2>/dev/null; then
    echo "FAIL: merged.json is not a JSON array"
    exit 1
fi

# Check if it has 4 items
item_count=$(python3 -c "import json; print(len(json.load(open('merged.json'))))" 2>/dev/null)
if [ "$item_count" -ne 4 ]; then
    echo "FAIL: Expected 4 items, got $item_count"
    exit 1
fi

# Check if first item is Alice
first_name=$(python3 -c "import json; print(json.load(open('merged.json'))[0]['name'])" 2>/dev/null)
if [ "$first_name" != "Alice" ]; then
    echo "FAIL: Expected first item to be Alice, got $first_name"
    exit 1
fi

# Check if last item is Diana
last_name=$(python3 -c "import json; print(json.load(open('merged.json'))[3]['name'])" 2>/dev/null)
if [ "$last_name" != "Diana" ]; then
    echo "FAIL: Expected last item to be Diana, got $last_name"
    exit 1
fi

echo "PASS: JSON files merged correctly"
exit 0
