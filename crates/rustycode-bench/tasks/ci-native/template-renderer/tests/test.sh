#!/bin/bash
set -e

if [ ! -f "render.py" ]; then
    echo "FAIL: render.py not found"
    exit 1
fi

# Run the renderer
output=$(python3 render.py template.txt data.json)

# Check key substitutions
if ! echo "$output" | grep -q "Jane Doe"; then
    echo "FAIL: Missing 'Jane Doe' in output"
    exit 1
fi

if ! echo "$output" | grep -q "order #42"; then
    echo "FAIL: Missing 'order #42' in output"
    exit 1
fi

if ! echo "$output" | grep -q '\$99.95'; then
    echo "FAIL: Missing '\$99.95' in output"
    exit 1
fi

if ! echo "$output" | grep -q "123 Main St"; then
    echo "FAIL: Missing address in output"
    exit 1
fi

# Check missing key replaced with empty string (tracking_number not in data.json)
if echo "$output" | grep -q "{{tracking_number}}"; then
    echo "FAIL: Unresolved placeholder {{tracking_number}} should be replaced with empty string"
    exit 1
fi

# Check greeting uses user.name correctly (appears twice)
name_count=$(echo "$output" | grep -c "Jane Doe")
if [ "$name_count" -lt 2 ]; then
    echo "FAIL: Expected 'Jane Doe' at least twice (greeting and closing)"
    exit 1
fi

# Verify no remaining placeholders
if echo "$output" | grep -q '{{.*}}'; then
    echo "FAIL: Unresolved placeholders remain in output"
    exit 1
fi

echo "PASS: template renderer works correctly"
exit 0
