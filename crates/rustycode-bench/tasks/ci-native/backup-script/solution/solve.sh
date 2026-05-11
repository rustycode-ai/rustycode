#!/bin/bash
set -e
cat > backup.sh << 'SCRIPT'
#!/bin/bash
mkdir -p backups
pattern="$1"
ts=$(date +%Y%m%d_%H%M%S)
for f in $pattern; do
    [ -f "$f" ] && cp "$f" "backups/${ts}_${f}"
done
SCRIPT
chmod +x backup.sh
