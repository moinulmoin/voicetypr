#!/bin/bash
set -e

# Configuration
KEY_PATH="$HOME/.tauri/voicetypr.key"
RELEASE_DIR="release-1.0.0"
TAR_FILE="VoiceTypr_1.0.0_universal.app.tar.gz"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Tauri Updater Signing Script${NC}"
echo "================================"

# Check if rsign is installed
if ! command -v rsign &> /dev/null; then
    echo -e "${YELLOW}rsign is not installed. Installing...${NC}"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # Try cargo first
        if command -v cargo &> /dev/null; then
            cargo install rsign2
        else
            echo -e "${RED}Error: Neither rsign nor cargo is installed${NC}"
            echo "Install Rust first: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
            exit 1
        fi
    fi
fi

# Check files exist
if [ ! -f "$KEY_PATH" ]; then
    echo -e "${RED}Error: Private key not found at $KEY_PATH${NC}"
    exit 1
fi

if [ ! -f "$RELEASE_DIR/$TAR_FILE" ]; then
    echo -e "${RED}Error: $TAR_FILE not found in $RELEASE_DIR${NC}"
    exit 1
fi

cd "$RELEASE_DIR"

# Method 1: Try with cargo tauri signer using password
echo -e "${YELLOW}Method 1: Trying cargo tauri signer with password...${NC}"
read -s -p "Enter password for the signing key: " PASSWORD
echo ""

export TAURI_SIGNING_PRIVATE_KEY=$(cat "$KEY_PATH")
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$PASSWORD"

if cargo tauri signer sign "$TAR_FILE" 2>/dev/null; then
    echo -e "${GREEN}✅ Success! Signature created with cargo tauri signer${NC}"
else
    echo -e "${YELLOW}cargo tauri signer failed, trying rsign...${NC}"
    
    # Method 2: Use rsign directly
    if command -v rsign &> /dev/null; then
        echo -e "${YELLOW}Method 2: Using rsign directly...${NC}"
        
        # rsign requires the password to be provided interactively or via stdin
        echo "$PASSWORD" | rsign sign -s "$KEY_PATH" "$TAR_FILE"
        
        if [ -f "${TAR_FILE}.sig" ]; then
            echo -e "${GREEN}✅ Success! Signature created with rsign${NC}"
        else
            echo -e "${RED}❌ rsign signing failed${NC}"
        fi
    fi
fi

# Check if signature was created
if [ -f "${TAR_FILE}.sig" ]; then
    echo -e "${GREEN}Signature created successfully!${NC}"
    echo "Signature content:"
    cat "${TAR_FILE}.sig"
    
    # Update latest.json with the signature
    SIGNATURE=$(cat "${TAR_FILE}.sig" | grep -v "untrusted comment" | tr -d '\n')
    echo ""
    echo -e "${YELLOW}Add this signature to latest.json:${NC}"
    echo "$SIGNATURE"
else
    echo -e "${RED}❌ Failed to create signature${NC}"
    echo ""
    echo "Alternative: You can generate the signature on another machine where it works,"
    echo "or try using the TAURI_SIGNING_PRIVATE_KEY_PASSWORD environment variable"
fi

cd -