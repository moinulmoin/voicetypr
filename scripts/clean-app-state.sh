#!/bin/bash

# Script to remove all VoiceTypr app states for clean testing
# Usage: ./scripts/clean-app-state.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${YELLOW}🧹 VoiceTypr State Cleanup Script${NC}"
echo "=================================="
echo ""

# App bundle identifier
APP_ID="com.ideaplexa.voicetypr"

# Check if VoiceTypr is running
if pgrep -x "voicetypr" > /dev/null; then
    echo -e "${RED}VoiceTypr is currently running!${NC}"
    echo "Please quit the app before cleaning state."
    exit 1
fi

echo -e "${YELLOW}This will remove:${NC}"
echo "• App preferences and settings"
echo "• Downloaded Whisper models"
echo "• Transcription history"
echo "• Cached data"
echo "• Keychain entries"
echo ""
read -p "Are you sure you want to continue? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

echo ""
echo -e "${YELLOW}Cleaning app state...${NC}"

# 1. Remove app preferences
echo -e "${BLUE}→ Removing preferences...${NC}"
defaults delete "$APP_ID" 2>/dev/null || true
rm -rf "$HOME/Library/Preferences/${APP_ID}.plist" 2>/dev/null || true

# 2. Remove app support data (includes Whisper models)
echo -e "${BLUE}→ Removing app support data...${NC}"
rm -rf "$HOME/Library/Application Support/${APP_ID}" 2>/dev/null || true

# 3. Remove caches
echo -e "${BLUE}→ Removing caches...${NC}"
rm -rf "$HOME/Library/Caches/${APP_ID}" 2>/dev/null || true
rm -rf "$HOME/Library/Caches/com.apple.nsurlsessiond/Downloads/${APP_ID}" 2>/dev/null || true

# 4. Remove saved state
echo -e "${BLUE}→ Removing saved state...${NC}"
rm -rf "$HOME/Library/Saved Application State/${APP_ID}.savedState" 2>/dev/null || true

# 5. Remove logs
echo -e "${BLUE}→ Removing logs...${NC}"
rm -rf "$HOME/Library/Logs/${APP_ID}" 2>/dev/null || true

# 6. Remove WebKit data if any
echo -e "${BLUE}→ Removing WebKit data...${NC}"
rm -rf "$HOME/Library/WebKit/${APP_ID}" 2>/dev/null || true

# 7. Remove keychain entries
echo -e "${BLUE}→ Removing keychain entries...${NC}"
security delete-generic-password -s "${APP_ID}" 2>/dev/null || true
security delete-generic-password -s "${APP_ID}.voicetypr" 2>/dev/null || true

# 8. Remove Tauri plugin data
echo -e "${BLUE}→ Removing Tauri plugin data...${NC}"
rm -rf "$HOME/Library/Application Support/${APP_ID}/plugins" 2>/dev/null || true

# 9. Kill cfprefsd to ensure preference changes take effect
echo -e "${BLUE}→ Refreshing preferences daemon...${NC}"
killall cfprefsd 2>/dev/null || true

echo ""
echo -e "${GREEN}✅ App state cleaned successfully!${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Launch VoiceTypr to see fresh onboarding"
echo "2. You'll need to:"
echo "   • Grant permissions again"
echo "   • Download Whisper models again"
echo "   • Reconfigure settings"
echo ""

# Optional: Show what would be left
echo -e "${BLUE}Checking for any remaining files...${NC}"
REMAINING=$(find "$HOME/Library" -name "*voicetypr*" -o -name "*${APP_ID}*" 2>/dev/null | grep -v "VoiceTypr.app" || true)
if [[ -n "$REMAINING" ]]; then
    echo -e "${YELLOW}Found some remaining files:${NC}"
    echo "$REMAINING"
else
    echo -e "${GREEN}No remaining files found.${NC}"
fi