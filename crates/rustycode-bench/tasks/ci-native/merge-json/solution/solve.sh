#!/bin/bash
set -e
python3 -c "
import json
with open('users1.json') as f:
    u1 = json.load(f)
with open('users2.json') as f:
    u2 = json.load(f)
merged = u1 + u2
with open('merged.json', 'w') as f:
    json.dump(merged, f, indent=2)
"
