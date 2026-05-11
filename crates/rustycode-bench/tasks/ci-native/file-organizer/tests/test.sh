#!/bin/bash
set -e

# Create test files
echo "test notes" > notes.txt
echo "python script" > script.py
echo "javascript app" > app.js
echo "bash script" > run.sh
echo "json data" > data.json

# Run the organizer script
if [ ! -f "organize.sh" ]; then
    echo "FAIL: organize.sh not found"
    exit 1
fi

chmod +x organize.sh
./organize.sh

# Check if directories were created
if [ ! -d "text" ]; then
    echo "FAIL: text/ directory not created"
    exit 1
fi

if [ ! -d "python" ]; then
    echo "FAIL: python/ directory not created"
    exit 1
fi

if [ ! -d "javascript" ]; then
    echo "FAIL: javascript/ directory not created"
    exit 1
fi

if [ ! -d "scripts" ]; then
    echo "FAIL: scripts/ directory not created"
    exit 1
fi

# Check if files were moved correctly
if [ ! -f "text/notes.txt" ]; then
    echo "FAIL: notes.txt not moved to text/"
    exit 1
fi

if [ ! -f "python/script.py" ]; then
    echo "FAIL: script.py not moved to python/"
    exit 1
fi

if [ ! -f "javascript/app.js" ]; then
    echo "FAIL: app.js not moved to javascript/"
    exit 1
fi

if [ ! -f "scripts/run.sh" ]; then
    echo "FAIL: run.sh not moved to scripts/"
    exit 1
fi

# Check if data.json was not moved
if [ -f "data.json" ]; then
    # Should still be in current directory
    :
else
    echo "FAIL: data.json should not have been moved"
    exit 1
fi

# Test idempotence - running again should not cause errors
./organize.sh

echo "PASS: Files organized correctly"
exit 0
