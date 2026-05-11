#!/bin/bash
set -e
cat > render.py << 'PYEOF'
import json
import re
import sys


def get_nested(data, key):
    parts = key.split(".")
    val = data
    for p in parts:
        if isinstance(val, dict) and p in val:
            val = val[p]
        else:
            return ""
    return str(val)


template_file = sys.argv[1]
data_file = sys.argv[2]

with open(template_file) as f:
    template = f.read()
with open(data_file) as f:
    data = json.load(f)

result = re.sub(r"\{\{(\w[\w.]*)\}\}", lambda m: get_nested(data, m.group(1)), template)
print(result)
PYEOF
