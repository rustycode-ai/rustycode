#!/bin/bash
set -e
cat > organize.sh << 'SCRIPT'
#!/bin/bash
mkdir -p text python javascript scripts
for f in *.txt; do [ -f "$f" ] && mv "$f" text/; done
for f in *.py; do [ -f "$f" ] && mv "$f" python/; done
for f in *.js; do [ -f "$f" ] && mv "$f" javascript/; done
for f in *.sh; do [ -f "$f" ] && [ "$f" != "organize.sh" ] && mv "$f" scripts/; done
SCRIPT
chmod +x organize.sh
