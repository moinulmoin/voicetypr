#!/usr/bin/env bash

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Default to dev bundle identifier; override by setting
# VOICETYPR_APP_ID if you need to target a different app.
APP_ID="${VOICETYPR_APP_ID:-com.ideaplexa.voicetypr.dev}"

echo -e "${YELLOW}Quick app state reset${NC}"

# Kill app if running (dev convenience)
pkill -x voicetypr 2>/dev/null || true

echo "→ Resetting preferences..."
defaults delete "$APP_ID" 2>/dev/null || true

echo "→ Clearing saved state..."
rm -rf "$HOME/Library/Saved Application State/${APP_ID}.savedState" 2>/dev/null || true

echo "→ Clearing app state files (keeping models)..."
rm -f "$HOME/Library/Application Support/${APP_ID}/state.json" 2>/dev/null || true
rm -f "$HOME/Library/Application Support/${APP_ID}/settings.json" 2>/dev/null || true

echo "→ Clearing keychain entry..."
security delete-generic-password -s "${APP_ID}" 2>/dev/null || true

killall cfprefsd 2>/dev/null || true

echo -e "${GREEN}✅ Reset complete!${NC}"
echo "Models kept, onboarding will show again."
