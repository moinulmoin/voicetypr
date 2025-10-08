#!/bin/bash

# Script to rebuild current release archives without version bump
# This fixes the AppleDouble files issue for already published releases

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Get current version from package.json
CURRENT_VERSION=$(node -p "require('./package.json').version")
echo -e "${GREEN}🔧 Rebuilding v${CURRENT_VERSION} with fixed archives${NC}"

# Set environment to prevent AppleDouble files
export COPYFILE_DISABLE=1

# Build for both architectures
echo -e "${GREEN}🔨 Building for both architectures...${NC}"
echo -e "${YELLOW}Building for Intel (x86_64)...${NC}"
cd src-tauri
cargo tauri build --target x86_64-apple-darwin --bundles app --config tauri.macos.conf.json
cd ..

echo -e "${YELLOW}Building for Apple Silicon (aarch64)...${NC}"
cd src-tauri
cargo tauri build --target aarch64-apple-darwin --bundles app --config tauri.macos.conf.json
cd ..

# Create output directory
OUTPUT_DIR="release-${CURRENT_VERSION}-fixed"
mkdir -p "$OUTPUT_DIR"

# Create fixed tar.gz archives
echo -e "${YELLOW}Creating fixed archives...${NC}"

# x86_64
X86_APP_DIR="src-tauri/target/x86_64-apple-darwin/release/bundle/macos"
if [[ -d "$X86_APP_DIR/voicetypr.app" ]]; then
    cd "$X86_APP_DIR"
    COPYFILE_DISABLE=1 tar -czf "VoiceTypr_${CURRENT_VERSION}_x64.app.tar.gz" --exclude='._*' --exclude='.DS_Store' voicetypr.app
    cd - > /dev/null
    cp "$X86_APP_DIR/VoiceTypr_${CURRENT_VERSION}_x64.app.tar.gz" "$OUTPUT_DIR/"
    echo -e "${GREEN}✓ Created x64 archive${NC}"
fi

# aarch64
AARCH64_APP_DIR="src-tauri/target/aarch64-apple-darwin/release/bundle/macos"
if [[ -d "$AARCH64_APP_DIR/voicetypr.app" ]]; then
    cd "$AARCH64_APP_DIR"
    COPYFILE_DISABLE=1 tar -czf "VoiceTypr_${CURRENT_VERSION}_aarch64.app.tar.gz" --exclude='._*' --exclude='.DS_Store' voicetypr.app
    cd - > /dev/null
    cp "$AARCH64_APP_DIR/VoiceTypr_${CURRENT_VERSION}_aarch64.app.tar.gz" "$OUTPUT_DIR/"
    echo -e "${GREEN}✓ Created aarch64 archive${NC}"
fi

# Sign the archives if signing key is available
TAURI_KEY_PATH="$HOME/.tauri/voicetypr.key"
if [[ -f "$TAURI_KEY_PATH" ]] || [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]] || [[ -n "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
    echo -e "${YELLOW}Signing archives...${NC}"
    
    # Set key path if not already set
    if [[ -f "$TAURI_KEY_PATH" ]] && [[ -z "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
        export TAURI_SIGNING_PRIVATE_KEY_PATH="$TAURI_KEY_PATH"
    fi
    
    # Sign x64
    if [[ -f "$OUTPUT_DIR/VoiceTypr_${CURRENT_VERSION}_x64.app.tar.gz" ]]; then
        if [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]]; then
            cargo tauri signer sign -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" -p "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" "$OUTPUT_DIR/VoiceTypr_${CURRENT_VERSION}_x64.app.tar.gz"
        else
            cargo tauri signer sign -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" -p "" "$OUTPUT_DIR/VoiceTypr_${CURRENT_VERSION}_x64.app.tar.gz"
        fi
    fi
    
    # Sign aarch64
    if [[ -f "$OUTPUT_DIR/VoiceTypr_${CURRENT_VERSION}_aarch64.app.tar.gz" ]]; then
        if [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]]; then
            cargo tauri signer sign -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" -p "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" "$OUTPUT_DIR/VoiceTypr_${CURRENT_VERSION}_aarch64.app.tar.gz"
        else
            cargo tauri signer sign -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" -p "" "$OUTPUT_DIR/VoiceTypr_${CURRENT_VERSION}_aarch64.app.tar.gz"
        fi
    fi
fi

# Create updated latest.json
echo -e "${YELLOW}Creating updated latest.json...${NC}"
node scripts/create-latest-json.js "$CURRENT_VERSION" "$OUTPUT_DIR"

echo -e "${GREEN}✅ Rebuild complete!${NC}"
echo -e "${GREEN}📁 Fixed artifacts saved in: ${OUTPUT_DIR}/${NC}"
echo ""
echo -e "${YELLOW}Files created:${NC}"
ls -la "$OUTPUT_DIR"
echo ""
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Test the updater with one of the fixed .app.tar.gz files"
echo "2. Upload all files to the existing v${CURRENT_VERSION} release:"
echo -e "${GREEN}   gh release upload v${CURRENT_VERSION} ${OUTPUT_DIR}/* --clobber${NC}"
echo ""
echo -e "${YELLOW}⚠️  The --clobber flag will overwrite existing files${NC}"