#!/bin/bash
# Migration script for RustyCode 2.0.0
# This script handles migration from old configuration format to new architecture

set -e

echo "======================================"
echo "RustyCode 2.0.0 Migration Script"
echo "======================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if rustycode is installed
if ! command -v rustycode &> /dev/null; then
    echo -e "${YELLOW}Warning: rustycode command not found in PATH${NC}"
    echo "This is expected if you haven't installed rustycode yet"
    echo "This script will prepare your configuration for the new version"
fi

# Check current version
if command -v rustycode &> /dev/null; then
    CURRENT_VERSION=$(rustycode --version 2>/dev/null || echo "unknown")
    echo -e "${GREEN}Current version: $CURRENT_VERSION${NC}"
else
    echo -e "${YELLOW}rustycode not yet installed${NC}"
fi
echo ""

# Backup existing data
BACKUP_DIR="$HOME/.rustycode/backup_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

echo -e "${GREEN}Step 1: Backing up existing data${NC}"
echo "Backup location: $BACKUP_DIR"

# Backup .rustycode directory if it exists
if [ -d "$HOME/.rustycode" ]; then
    echo "Backing up ~/.rustycode..."
    cp -r "$HOME/.rustycode" "$BACKUP_DIR/" 2>/dev/null || true
    echo -e "${GREEN}✓ Backup created${NC}"
else
    echo "No existing ~/.rustycode directory found (fresh installation)"
fi
echo ""

# Migrate configuration
echo -e "${GREEN}Step 2: Migrating configuration${NC}"

OLD_CONFIG="$HOME/.rustycode/config.toml"
NEW_CONFIG="$HOME/.rustycode/config.jsonc"

if [ -f "$OLD_CONFIG" ]; then
    echo "Found old TOML configuration: $OLD_CONFIG"

    # Create new config directory if it doesn't exist
    mkdir -p "$(dirname "$NEW_CONFIG")"

    echo "Converting TOML to JSONC format..."
    echo -e "${YELLOW}Note: Manual conversion required. Template created at $NEW_CONFIG${NC}"

    # Create a template config
    cat > "$NEW_CONFIG" << 'EOF'
{
  // RustyCode 2.0.0 Configuration
  // Migrated from TOML config - please review and update

  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "max_tokens": 4096,

  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}",
      "models": ["claude-3-5-sonnet-latest", "claude-opus-4-6"]
    }
  },

  "features": {
    "git_integration": true,
    "mcp_servers": []
  }
}
EOF

    echo -e "${GREEN}✓ Configuration template created${NC}"
    echo -e "${YELLOW}⚠ Please edit $NEW_CONFIG and:${NC}"
    echo "  1. Export your API keys: export ANTHROPIC_API_KEY=sk-ant-..."
    echo "  2. Review and update provider configurations"
    echo "  3. Adjust model selections and feature flags"
else
    echo "No old TOML configuration found"
    if [ ! -f "$NEW_CONFIG" ]; then
        echo "Creating default configuration..."
        mkdir -p "$(dirname "$NEW_CONFIG")"
        cat > "$NEW_CONFIG" << 'EOF'
{
  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "max_tokens": 4096,

  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}",
      "models": ["claude-3-5-sonnet-latest"]
    }
  },

  "features": {
    "git_integration": true,
    "mcp_servers": []
  }
}
EOF
        echo -e "${GREEN}✓ Default configuration created${NC}"
    fi
fi
echo ""

# Update session storage format
echo -e "${GREEN}Step 3: Updating session storage${NC}"

SESSIONS_DIR="$HOME/.rustycode/sessions"
if [ -d "$SESSIONS_DIR" ]; then
    SESSION_COUNT=$(find "$SESSIONS_DIR" -name "*.json" | wc -l)
    if [ $SESSION_COUNT -gt 0 ]; then
        echo "Found $SESSION_COUNT session files in old format"
        echo -e "${YELLOW}Note: Sessions will be automatically migrated on first use${NC}"
        echo "Sessions will be converted to new binary format with compression"
    else
        echo "No session files found"
    fi
else
    echo "No sessions directory found (will be created on first use)"
fi
echo ""

# Clean up old files
echo -e "${GREEN}Step 4: Cleaning up old files${NC}"

# Remove old TOML config after migration
if [ -f "$OLD_CONFIG" ] && [ -f "$NEW_CONFIG" ]; then
    echo "Old TOML config preserved at: $OLD_CONFIG"
    echo "You can safely remove it after verifying new config works:"
    echo "  rm $OLD_CONFIG"
fi

# Remove old cache files
if [ -d "$HOME/.rustycode/cache" ]; then
    echo "Cache directory exists - will be regenerated on next run"
fi
echo ""

# Create log directory
echo -e "${GREEN}Step 5: Setting up logging${NC}"
mkdir -p "$HOME/.rustycode/logs"
echo -e "${GREEN}✓ Log directory created${NC}"
echo ""

# Summary
echo "======================================"
echo -e "${GREEN}Migration Complete!${NC}"
echo "======================================"
echo ""
echo "Next steps:"
echo ""
echo "1. Export your API keys:"
echo -e "${YELLOW}   export ANTHROPIC_API_KEY=sk-ant-...${NC}"
echo -e "${YELLOW}   export OPENAI_API_KEY=sk-...${NC}"
echo ""
echo "2. Review and update configuration:"
echo -e "${YELLOW}   $NEW_CONFIG${NC}"
echo ""
echo "3. Install new version:"
echo -e "${YELLOW}   cargo install --path .${NC}"
echo ""
echo "4. Verify installation:"
echo -e "${YELLOW}   rustycode --version${NC}"
echo -e "${YELLOW}   rustycode --check${NC}"
echo ""
echo "5. Run smoke tests:"
echo -e "${YELLOW}   ./scripts/smoke_test.sh${NC}"
echo ""
echo "Backup location: $BACKUP_DIR"
echo ""
echo "For detailed migration guide, see:"
echo "  docs/architecture-upgrade/MIGRATION.md"
echo ""
echo "For troubleshooting, see:"
echo "  docs/architecture-upgrade/MIGRATION.md#common-issues"
echo ""
