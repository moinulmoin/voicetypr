#!/bin/bash
set -e

# Configuration
KEY_PATH="$HOME/.tauri/voicetypr.key"
RELEASE_DIR="release-1.0.0"
TAR_FILE="VoiceTypr_1.0.0_universal.app.tar.gz"
PASSWORD="112468"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${YELLOW}Tauri Updater Signing Process${NC}"
echo "=============================="

# Check prerequisites
if [ ! -f "$KEY_PATH" ]; then
    echo -e "${RED}Error: Private key not found at $KEY_PATH${NC}"
    exit 1
fi

if [ ! -f "$RELEASE_DIR/$TAR_FILE" ]; then
    echo -e "${RED}Error: $TAR_FILE not found in $RELEASE_DIR${NC}"
    exit 1
fi

# Read the key content (not the path)
PRIVATE_KEY_CONTENT=$(cat "$KEY_PATH")

# Export environment variables with the actual key content
export TAURI_SIGNING_PRIVATE_KEY="$PRIVATE_KEY_CONTENT"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$PASSWORD"

echo "Key loaded and password set"
echo ""

# Navigate to release directory
cd "$RELEASE_DIR"

# Remove any existing signature
rm -f "${TAR_FILE}.sig"

echo "Signing $TAR_FILE..."
echo ""

# Use cargo tauri signer sign
if cargo tauri signer sign "$TAR_FILE"; then
    echo -e "${GREEN}✅ Signature created successfully!${NC}"
    
    if [ -f "${TAR_FILE}.sig" ]; then
        echo ""
        echo "Signature file: ${TAR_FILE}.sig"
        
        # Read the signature
        SIGNATURE=$(cat "${TAR_FILE}.sig")
        echo "Signature content:"
        echo "$SIGNATURE"
        
        # Update latest.json with the signature
        echo ""
        echo -e "${YELLOW}Updating latest.json with signature...${NC}"
        
        # Use jq if available, otherwise manual update needed
        if command -v jq &> /dev/null; then
            jq --arg sig "$SIGNATURE" '.platforms."darwin-universal".signature = $sig' latest.json > latest.json.tmp && mv latest.json.tmp latest.json
            echo -e "${GREEN}✅ latest.json updated${NC}"
        else
            echo "Please manually update latest.json with this signature:"
            echo "$SIGNATURE"
        fi
    fi
else
    echo -e "${RED}❌ Signing failed${NC}"
    echo ""
    echo "Debug: First line of key file:"
    head -n 1 "$KEY_PATH"
    echo ""
    echo "This might mean:"
    echo "1. The password is incorrect"
    echo "2. The key format is not compatible"
    echo "3. cargo tauri signer needs to be updated"
fi

cd -

# Clean up environment
unset TAURI_SIGNING_PRIVATE_KEY
unset TAURI_SIGNING_PRIVATE_KEY_PASSWORD

echo ""
echo "Done!"