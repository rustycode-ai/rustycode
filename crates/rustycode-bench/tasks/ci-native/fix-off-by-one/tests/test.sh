#!/bin/bash
set -e

if [ ! -f "search.py" ]; then
    echo "FAIL: search.py not found"
    exit 1
fi

# Test: find element in middle
result=$(python3 -c "from search import search; print(search([1, 3, 5, 7, 9], 5))")
if [ "$result" != "2" ]; then
    echo "FAIL: search([1,3,5,7,9], 5) returned $result, expected 2"
    exit 1
fi

# Test: find first element
result=$(python3 -c "from search import search; print(search([1, 3, 5, 7, 9], 1))")
if [ "$result" != "0" ]; then
    echo "FAIL: search([1,3,5,7,9], 1) returned $result, expected 0"
    exit 1
fi

# Test: find last element
result=$(python3 -c "from search import search; print(search([1, 3, 5, 7, 9], 9))")
if [ "$result" != "4" ]; then
    echo "FAIL: search([1,3,5,7,9], 9) returned $result, expected 4"
    exit 1
fi

# Test: element not found
result=$(python3 -c "from search import search; print(search([1, 3, 5, 7, 9], 4))")
if [ "$result" != "-1" ]; then
    echo "FAIL: search([1,3,5,7,9], 4) returned $result, expected -1"
    exit 1
fi

# Test: single element found
result=$(python3 -c "from search import search; print(search([42], 42))")
if [ "$result" != "0" ]; then
    echo "FAIL: search([42], 42) returned $result, expected 0"
    exit 1
fi

echo "PASS: binary search fixed correctly"
exit 0
