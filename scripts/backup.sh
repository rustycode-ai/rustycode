#!/bin/bash
# Backup script for RustyCode deployment
# Creates backup of current installation before upgrade

set -e

echo "======================================"
echo "RustyCode 2.0.0 Backup Script"
echo "======================================"
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Create backup directory with timestamp
BACKUP_DIR="$HOME/.rustycode/backup_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

echo -e "${GREEN}Creating backup at: $BACKUP_DIR${NC}"
echo ""

# Backup .rustycode directory
echo "Backing up ~/.rustycode directory..."
if [ -d "$HOME/.rustycode" ]; then
    cp -r "$HOME/.rustycode" "$BACKUP_DIR/"
    echo -e "${GREEN}✓ Configuration and data backed up${NC}"
else
    echo "No .rustycode directory found (skipping)"
fi
echo ""

# Backup binary
echo "Backing up rustycode binary..."
if command -v rustycode &> /dev/null; then
    BINARY_PATH=$(which rustycode)
    if [ -f "$BINARY_PATH" ]; then
        cp "$BINARY_PATH" "$BACKUP_DIR/rustycode-binary"
        echo -e "${GREEN}✓ Binary backed up from: $BINARY_PATH${NC}"
    fi
else
    echo "No rustycode binary found (skipping)"
fi
echo ""

# Backup current configuration
echo "Backing up configuration files..."
if [ -f "$HOME/.rustycode/config.toml" ]; then
    cp "$HOME/.rustycode/config.toml" "$BACKUP_DIR/config.toml.backup"
    echo -e "${GREEN}✓ TOML configuration backed up${NC}"
fi

if [ -f "$HOME/.rustycode/config.jsonc" ]; then
    cp "$HOME/.rustycode/config.jsonc" "$BACKUP_DIR/config.jsonc.backup"
    echo -e "${GREEN}✓ JSONC configuration backed up${NC}"
fi
echo ""

# Create backup manifest
echo "Creating backup manifest..."
cat > "$BACKUP_DIR/backup-info.txt" << EOF
RustyCode Backup
================
Created: $(date)
Backup Directory: $BACKUP_DIR

Contents:
EOF

if [ -d "$BACKUP_DIR/.rustycode" ]; then
    echo "- .rustycode directory" >> "$BACKUP_DIR/backup-info.txt"
fi

if [ -f "$BACKUP_DIR/rustycode-binary" ]; then
    echo "- rustycode binary" >> "$BACKUP_DIR/backup-info.txt"
fi

if [ -f "$BACKUP_DIR/config.toml.backup" ]; then
    echo "- TOML configuration" >> "$BACKUP_DIR/backup-info.txt"
fi

if [ -f "$BACKUP_DIR/config.jsonc.backup" ]; then
    echo "- JSONC configuration" >> "$BACKUP_DIR/backup-info.txt"
fi

echo "" >> "$BACKUP_DIR/backup-info.txt"
echo "To restore this backup, run:" >> "$BACKUP_DIR/backup-info.txt"
echo "  ./scripts/rollback_deployment.sh $BACKUP_DIR" >> "$BACKUP_DIR/backup-info.txt"

echo -e "${GREEN}✓ Backup manifest created${NC}"
echo ""

# Show backup summary
echo "======================================"
echo -e "${GREEN}Backup Complete!${NC}"
echo "======================================"
echo ""
echo "Backup location: $BACKUP_DIR"
echo ""
echo "Backup contents:"
ls -lh "$BACKUP_DIR"
echo ""
echo "To restore this backup:"
echo -e "${YELLOW}  ./scripts/rollback_deployment.sh $BACKUP_DIR${NC}"
echo ""
echo "Next steps:"
echo "1. Run migration script: ./scripts/migrate.sh"
echo "2. Install new version: cargo install --path ."
echo "3. Run smoke tests: ./scripts/smoke_test.sh"
echo ""
