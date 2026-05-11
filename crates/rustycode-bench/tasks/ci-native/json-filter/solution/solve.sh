#!/bin/bash
set -e
python3 -c "
import json
with open('users.json') as f:
    users = json.load(f)
filtered = [{'name': u['name'], 'email': u['email']} for u in users if u.get('active') and u.get('age', 0) >= 30]
filtered.sort(key=lambda x: x['name'])
with open('active_users.json', 'w') as f:
    json.dump(filtered, f, indent=2)
"
