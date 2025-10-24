#!/bin/bash

# Release script for VoiceTypr
# Usage: ./scripts/release.sh [patch|minor|major]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if release type is provided
RELEASE_TYPE=${1:-patch}
if [[ ! "$RELEASE_TYPE" =~ ^(patch|minor|major)$ ]]; then
    echo -e "${RED}Error: Invalid release type. Use: patch, minor, or major${NC}"
    exit 1
fi

echo -e "${GREEN}🚀 Starting VoiceTypr release process (${RELEASE_TYPE})${NC}"

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

# Build for both architectures
echo -e "${GREEN}🔨 Building for both architectures...${NC}"
echo -e "${YELLOW}Building for Intel (x86_64)...${NC}"
cd src-tauri
cargo tauri build --target x86_64-apple-darwin --config tauri.macos.conf.json

echo -e "${YELLOW}Building for Apple Silicon (aarch64)...${NC}"
cargo tauri build --target aarch64-apple-darwin --config tauri.macos.conf.json
cd ..

# Fix archives to remove AppleDouble files
echo -e "${YELLOW}Fixing archives to prevent updater errors...${NC}"
./scripts/fix-release-archives.sh

# Create output directory
OUTPUT_DIR="release-${NEW_VERSION}"
mkdir -p "$OUTPUT_DIR"

# Copy artifacts
echo -e "${YELLOW}Collecting artifacts...${NC}"
cp "src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/"*.dmg "$OUTPUT_DIR/" 2>/dev/null || true
cp "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/"*.dmg "$OUTPUT_DIR/" 2>/dev/null || true
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

### First-time Setup
1. Download the appropriate DMG for your Mac
2. Open the DMG and drag VoiceTypr to Applications
3. Right-click and select "Open" to bypass Gatekeeper
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

echo -e "${GREEN}✅ Release process complete!${NC}"
echo -e "${GREEN}📁 Artifacts saved in: ${OUTPUT_DIR}/${NC}"
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Review the draft release on GitHub"
echo "2. Test the downloaded artifacts"
echo "3. Publish the release when ready"
echo ""
echo -e "${GREEN}🔗 Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/v${NEW_VERSION}${NC}"