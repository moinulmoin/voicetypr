#!/bin/bash

# Release script for VoiceTypr with Apple Notarization
# Usage: ./scripts/release-notarized.sh [patch|minor|major]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if release type is provided
RELEASE_TYPE=${1:-patch}
if [[ ! "$RELEASE_TYPE" =~ ^(patch|minor|major)$ ]]; then
    echo -e "${RED}Error: Invalid release type. Use: patch, minor, or major${NC}"
    exit 1
fi

# Check for required environment variables
echo -e "${YELLOW}Checking Apple credentials...${NC}"
if [[ -z "$APPLE_ID" || -z "$APPLE_APP_PASSWORD" || -z "$APPLE_TEAM_ID" || -z "$APPLE_SIGNING_IDENTITY" ]]; then
    echo -e "${RED}Error: Missing Apple credentials in environment${NC}"
    echo "Please set the following environment variables:"
    echo "  export APPLE_ID='your@email.com'"
    echo "  export APPLE_APP_PASSWORD='xxxx-xxxx-xxxx-xxxx'"
    echo "  export APPLE_TEAM_ID='XXXXXXXXXX'"
    echo "  export APPLE_SIGNING_IDENTITY='Developer ID Application: Your Name (TEAMID)'"
    exit 1
fi

echo -e "${GREEN}✓ Apple credentials found${NC}"
echo -e "  Apple ID: ${APPLE_ID}"
echo -e "  Team ID: ${APPLE_TEAM_ID}"
echo -e "  Identity: ${APPLE_SIGNING_IDENTITY}"

echo -e "${GREEN}🚀 Starting VoiceTypr release process with notarization (${RELEASE_TYPE})${NC}"

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" != "main" ]]; then
    echo -e "${YELLOW}Warning: Not on main branch (currently on ${CURRENT_BRANCH})${NC}"
    read -p "Continue anyway? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check for uncommitted changes
if [[ -n $(git status -s) ]]; then
    echo -e "${RED}Error: You have uncommitted changes${NC}"
    git status -s
    exit 1
fi

# Pull latest changes
echo -e "${YELLOW}Pulling latest changes...${NC}"
git pull origin main

# Run tests first
echo -e "${YELLOW}Running tests...${NC}"
pnpm test
cd src-tauri && cargo test && cd ..

# Get current version
CURRENT_VERSION=$(node -p "require('./package.json').version")
echo -e "${GREEN}Current version: ${CURRENT_VERSION}${NC}"

# Use release-it to handle version bump and changelog
echo -e "${YELLOW}Running release-it...${NC}"
npx release-it $RELEASE_TYPE --ci

# Get new version
NEW_VERSION=$(node -p "require('./package.json').version")
echo -e "${GREEN}New version: ${NEW_VERSION}${NC}"

# Update version in tauri.conf.json
echo -e "${YELLOW}Updating tauri.conf.json...${NC}"
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/\"version\": \".*\"/\"version\": \"${NEW_VERSION}\"/" src-tauri/tauri.conf.json
else
    sed -i "s/\"version\": \".*\"/\"version\": \"${NEW_VERSION}\"/" src-tauri/tauri.conf.json
fi

# Commit the tauri.conf.json change
git add src-tauri/tauri.conf.json
git commit -m "chore: update tauri.conf.json version to ${NEW_VERSION}"

# Push changes and tags
echo -e "${YELLOW}Pushing to GitHub...${NC}"
git push origin main
git push origin "v${NEW_VERSION}"

# Function to sign and notarize a DMG
notarize_dmg() {
    local DMG_PATH=$1
    local ARCH=$2
    
    echo -e "${BLUE}📝 Signing ${ARCH} DMG...${NC}"
    
    # Sign the DMG
    codesign --force --deep --options runtime --sign "$APPLE_SIGNING_IDENTITY" "$DMG_PATH"
    
    echo -e "${BLUE}🚀 Submitting ${ARCH} DMG for notarization...${NC}"
    
    # Submit for notarization
    xcrun notarytool submit "$DMG_PATH" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_APP_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait \
        --verbose
    
    echo -e "${BLUE}📎 Stapling notarization to ${ARCH} DMG...${NC}"
    
    # Staple the notarization
    xcrun stapler staple "$DMG_PATH"
    
    # Verify
    echo -e "${BLUE}✅ Verifying ${ARCH} DMG notarization...${NC}"
    spctl -a -t open --context context:primary-signature -v "$DMG_PATH"
    
    echo -e "${GREEN}✓ ${ARCH} DMG notarized successfully!${NC}"
}

# Build for both architectures
echo -e "${GREEN}🔨 Building for both architectures...${NC}"

# Build Intel version
echo -e "${YELLOW}Building for Intel (x86_64)...${NC}"
cd src-tauri
cargo tauri build --target x86_64-apple-darwin --config tauri.macos.conf.json
cd ..

# Sign and notarize Intel build
INTEL_DMG=$(find "src-tauri/target/x86_64-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
if [[ -n "$INTEL_DMG" ]]; then
    notarize_dmg "$INTEL_DMG" "Intel"
else
    echo -e "${RED}Error: Intel DMG not found${NC}"
    exit 1
fi

# Build Apple Silicon version
echo -e "${YELLOW}Building for Apple Silicon (aarch64)...${NC}"
cd src-tauri
cargo tauri build --target aarch64-apple-darwin --config tauri.macos.conf.json
cd ..

# Sign and notarize Apple Silicon build
SILICON_DMG=$(find "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
if [[ -n "$SILICON_DMG" ]]; then
    notarize_dmg "$SILICON_DMG" "Apple Silicon"
else
    echo -e "${RED}Error: Apple Silicon DMG not found${NC}"
    exit 1
fi

# Create output directory
OUTPUT_DIR="release-${NEW_VERSION}"
mkdir -p "$OUTPUT_DIR"

# Copy artifacts
echo -e "${YELLOW}Collecting artifacts...${NC}"
cp "$INTEL_DMG" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_x64.dmg"
cp "$SILICON_DMG" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.dmg"
cp "src-tauri/target/x86_64-apple-darwin/release/bundle/macos/"*.app.tar.gz* "$OUTPUT_DIR/" 2>/dev/null || true
cp "src-tauri/target/aarch64-apple-darwin/release/bundle/macos/"*.app.tar.gz* "$OUTPUT_DIR/" 2>/dev/null || true

# Create combined latest.json
echo -e "${YELLOW}Creating combined latest.json...${NC}"
node scripts/create-latest-json.js "$NEW_VERSION" "$OUTPUT_DIR"

# Create GitHub release using gh CLI
echo -e "${YELLOW}Creating GitHub release draft...${NC}"
if command -v gh &> /dev/null; then
    CHANGELOG_CONTENT=$(sed -n "/## ${NEW_VERSION}/,/## [0-9]/p" CHANGELOG.md | sed '$ d')
    
    gh release create "v${NEW_VERSION}" \
        --draft \
        --title "VoiceTypr v${NEW_VERSION}" \
        --notes "$(cat <<EOF
$CHANGELOG_CONTENT

## Installation

### macOS
- **Intel Mac**: Download \`VoiceTypr_${NEW_VERSION}_x64.dmg\`
- **Apple Silicon**: Download \`VoiceTypr_${NEW_VERSION}_aarch64.dmg\`

### ✅ Fully Notarized
This release is signed and notarized by Apple. You can download and run VoiceTypr without any security warnings.

### First-time Setup
1. Download the appropriate DMG for your Mac
2. Open the DMG and drag VoiceTypr to Applications
3. Launch VoiceTypr normally (no right-click needed!)
4. Grant microphone and accessibility permissions when prompted

## Auto-Updates

VoiceTypr will automatically check for updates. You can also check manually in Settings → About.
EOF
)"
    
    echo -e "${GREEN}✅ Draft release created!${NC}"
    echo -e "${YELLOW}Uploading artifacts...${NC}"
    gh release upload "v${NEW_VERSION}" "$OUTPUT_DIR"/* --clobber
else
    echo -e "${YELLOW}GitHub CLI not found. Please install it with: brew install gh${NC}"
    echo -e "${YELLOW}Or manually create release at: https://github.com/moinulmoin/voicetypr/releases/new${NC}"
fi

echo -e "${GREEN}✅ Release process complete with notarization!${NC}"
echo -e "${GREEN}📁 Notarized artifacts saved in: ${OUTPUT_DIR}/${NC}"
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Review the draft release on GitHub"
echo "2. Test the notarized DMGs"
echo "3. Publish the release when ready"
echo ""
echo -e "${GREEN}🔗 Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/v${NEW_VERSION}${NC}"
echo -e "${GREEN}🎉 Your app is now fully notarized and ready for distribution!${NC}"