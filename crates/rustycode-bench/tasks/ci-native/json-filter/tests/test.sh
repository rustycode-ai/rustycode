#!/bin/bash
set -e

if [ ! -f "active_users.json" ]; then
    echo "FAIL: active_users.json not found"
    exit 1
fi

# Validate JSON
python3 -c "import json; json.load(open('active_users.json'))" || {
    echo "FAIL: active_users.json is not valid JSON"
    exit 1
}

# Check count: Alice(35,active), Charlie(31,active), Eve(38,active), Grace(30,active) = 4
count=$(python3 -c "import json; print(len(json.load(open('active_users.json'))))")
if [ "$count" != "4" ]; then
    echo "FAIL: Expected 4 users, got $count"
    exit 1
fi

# Check fields: only name and email
fields=$(python3 -c "
import json
users = json.load(open('active_users.json'))
keys = set()
for u in users:
    keys.update(u.keys())
print(','.join(sorted(keys)))
")
if [ "$fields" != "email,name" ]; then
    echo "FAIL: Expected only name,email fields, got: $fields"
    exit 1
fi

# Check sort order (alphabetical by name)
first=$(python3 -c "import json; print(json.load(open('active_users.json'))[0]['name'])")
if [ "$first" != "Alice" ]; then
    echo "FAIL: First entry should be Alice, got $first"
    exit 1
fi

last=$(python3 -c "import json; data=json.load(open('active_users.json')); print(data[-1]['name'])")
if [ "$last" != "Grace" ]; then
    echo "FAIL: Last entry should be Grace, got $last"
    exit 1
fi

echo "PASS: JSON filtered and transformed correctly"
exit 0
