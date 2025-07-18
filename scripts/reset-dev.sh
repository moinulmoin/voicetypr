#!/bin/bash

# Quick reset script for development testing
# Usage: ./scripts/reset-dev.sh

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

APP_ID="com.ideaplexa.voicetypr"

echo -e "${YELLOW}Quick development reset${NC}"

# Kill app if running
pkill -x voicetypr 2>/dev/null || true

# Just reset preferences and state, keep models
echo "→ Resetting preferences..."
defaults delete "$APP_ID" 2>/dev/null || true

echo "→ Clearing saved state..."
rm -rf "$HOME/Library/Saved Application State/${APP_ID}.savedState" 2>/dev/null || true

# Only clear app state, not models
rm -f "$HOME/Library/Application Support/${APP_ID}/state.json" 2>/dev/null || true
rm -f "$HOME/Library/Application Support/${APP_ID}/settings.json" 2>/dev/null || true

# Clear keychain
security delete-generic-password -s "${APP_ID}" 2>/dev/null || true

killall cfprefsd 2>/dev/null || true

echo -e "${GREEN}✅ Reset complete!${NC}"
echo "Models kept, onboarding will show again."