#!/bin/bash
set -e
cat > config.toml << 'EOF'
[server]
port = 8080
debug = true
log_path = "/var/log/app.log"
welcome_message = "Welcome to the server"

[database]
url = "postgres://localhost:5432/mydb"
pool_size = 10
EOF
