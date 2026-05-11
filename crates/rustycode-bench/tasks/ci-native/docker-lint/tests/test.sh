#!/bin/bash
set -e

if [ ! -f "config.toml" ]; then
    echo "FAIL: config.toml not found"
    exit 1
fi

# Verify it parses as valid TOML
python3 -c "
import tomllib
with open('config.toml', 'rb') as f:
    config = tomllib.load(f)

# Check duplicate key resolved to single value
port = config['server']['port']
assert isinstance(port, int), f'port should be int, got {type(port)}'

# Check debug is boolean, not string
debug = config['server']['debug']
assert isinstance(debug, bool), f'debug should be bool, got {type(debug)}'
assert debug is True, f'debug should be True, got {debug}'

# Check log_path doesn't have inline comment
log_path = config['server']['log_path']
assert '#' not in log_path, f'log_path should not contain #: {log_path}'

# Check welcome_message has closing quote
welcome = config['server']['welcome_message']
assert isinstance(welcome, str), 'welcome_message should be a string'
assert len(welcome) > 0, 'welcome_message should not be empty'

# Check database section preserved
assert config['database']['url'] == 'postgres://localhost:5432/mydb'
assert config['database']['pool_size'] == 10

print('OK')
" || {
    echo "FAIL: config.toml validation failed"
    exit 1
}

echo "PASS: configuration file fixed correctly"
exit 0
