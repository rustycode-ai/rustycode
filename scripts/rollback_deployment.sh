#!/bin/bash
# Rollback script for deployment issues
# Restores RustyCode to previous state from backup

set -e

echo "======================================"
echo "RustyCode 2.0.0 Rollback Script"
echo "======================================"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check backup directory argument
BACKUP_DIR="$1"

if [ -z "$BACKUP_DIR" ]; then
    echo -e "${RED}Error: Backup directory not specified${NC}"
    echo ""
    echo "Usage: $0 <backup_directory>"
    echo ""
    echo "Available backups:"
    ls -td ~/.rustycode/backup_* 2>/dev/null || echo "  No backups found"
    exit 1
fi

if [ ! -d "$BACKUP_DIR" ]; then
    echo -e "${RED}Error: Backup directory not found: $BACKUP_DIR${NC}"
    exit 1
fi

echo -e "${YELLOW}This will rollback RustyCode to previous state${NC}"
echo "Backup: $BACKUP_DIR"
echo ""
read -p "Are you sure? (yes/no): " -r
echo ""

if [ ! "$REPLY" = "yes" ]; then
    echo "Rollback cancelled"
    exit 0
fi

# Stop processes
echo -e "${GREEN}Step 1: Stopping rustycode processes${NC}"
if pgrep -f rustycode > /dev/null; then
    echo "Stopping running processes..."
    pkill -f rustycode || true
    sleep 2

    # Force kill if still running
    if pgrep -f rustycode > /dev/null; then
        echo "Force stopping..."
        pkill -9 -f rustycode || true
        sleep 1
    fi
    echo -e "${GREEN}✓ Processes stopped${NC}"
else
    echo "No rustycode processes running"
fi
echo ""

# Backup current state (just in case)
CURRENT_BACKUP="$HOME/.rustycode/pre_rollback_$(date +%Y%m%d_%H%M%S)"
echo -e "${GREEN}Step 2: Backing up current state${NC}"
echo "Creating backup at: $CURRENT_BACKUP"

if [ -d "$HOME/.rustycode" ]; then
    cp -r "$HOME/.rustycode" "$CURRENT_BACKUP/" 2>/dev/null || true
    echo -e "${GREEN}✓ Current state backed up${NC}"
fi
echo ""

# Restore data
echo -e "${GREEN}Step 3: Restoring data from backup${NC}"

if [ -d "$BACKUP_DIR/.rustycode" ]; then
    echo "Removing current .rustycode directory..."
    rm -rf "$HOME/.rustycode"

    echo "Restoring from backup..."
    cp -r "$BACKUP_DIR/.rustycode" "$HOME/"
    echo -e "${GREEN}✓ Data restored${NC}"
else
    echo -e "${YELLOW}Warning: No .rustycode in backup directory${NC}"
    echo "Skipping data restoration"
fi
echo ""

# Restore binary (if we backed it up)
echo -e "${GREEN}Step 4: Restoring binary${NC}"

BACKUP_BINARY="$BACKUP_DIR/rustycode-binary"
if [ -f "$BACKUP_BINARY" ]; then
    echo "Restoring rustycode binary..."
    # Determine binary location
    if [ -d "$HOME/.cargo/bin" ]; then
        CARGO_BIN="$HOME/.cargo/bin/rustycode"
        if [ -f "$CARGO_BIN" ]; then
            cp "$CARGO_BIN" "$CARGO_BIN.rollback"
            cp "$BACKUP_BINARY" "$CARGO_BIN"
            chmod +x "$CARGO_BIN"
            echo -e "${GREEN}✓ Binary restored (old binary saved to $CARGO_BIN.rollback)${NC}"
        fi
    fi
else
    echo -e "${YELLOW}Warning: No binary backup found${NC}"
    echo "You may need to manually reinstall:"
    echo "  cargo install --path ."
fi
echo ""

# Verify restoration
echo -e "${GREEN}Step 5: Verifying restoration${NC}"

if command -v rustycode &> /dev/null; then
    VERSION=$(rustycode --version 2>/dev/null || echo "unknown")
    echo -e "${GREEN}✓ rustycode command available${NC}"
    echo "Version: $VERSION"
else
    echo -e "${YELLOW}⚠ rustycode command not found${NC}"
    echo "You may need to reinstall:"
    echo "  cargo install --path ."
fi

if [ -d "$HOME/.rustycode" ]; then
    echo -e "${GREEN}✓ .rustycode directory exists${NC}"

    if [ -f "$HOME/.rustycode/config.toml" ] || [ -f "$HOME/.rustycode/config.jsonc" ]; then
        echo -e "${GREEN}✓ Configuration file exists${NC}"
    fi

    if [ -d "$HOME/.rustycode/sessions" ]; then
        SESSION_COUNT=$(find "$HOME/.rustycode/sessions" -type f | wc -l)
        echo -e "${GREEN}✓ Sessions directory exists ($SESSION_COUNT sessions)${NC}"
    fi
else
    echo -e "${RED}✗ .rustycode directory not found${NC}"
fi
echo ""

# Summary
echo "======================================"
echo -e "${GREEN}Rollback Complete!${NC}"
echo "======================================"
echo ""
echo "Restored from: $BACKUP_DIR"
echo "Current backup: $CURRENT_BACKUP"
echo ""
echo "Next steps:"
echo ""
echo "1. Verify rustycode works:"
echo -e "${YELLOW}   rustycode --version${NC}"
echo -e "${YELLOW}   rustycode --check${NC}"
echo ""
echo "2. Test basic functionality:"
echo -e "${YELLOW}   rustycode config show${NC}"
echo -e "${YELLOW}   rustycode providers list${NC}"
echo ""
echo "3. If issues persist:"
echo "  - Review logs: cat ~/.rustycode/logs/rustycode.log"
echo "  - Check configuration: rustycode config validate"
echo "  - Report issues with detailed error messages"
echo ""
echo "If you need to rollback again:"
echo "  Current backup available at: $CURRENT_BACKUP"
echo ""
echo "For troubleshooting, see:"
echo "  docs/architecture-upgrade/MIGRATION.md#common-issues"
echo ""
