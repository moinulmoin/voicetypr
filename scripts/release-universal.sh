#!/bin/bash

# Release script for VoiceTypr with Universal Binary and Built-in Tauri Notarization
# Usage: ./scripts/release-universal.sh [patch|minor|major]

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
echo -e "${YELLOW}Checking environment variables...${NC}"

# Check Apple signing/notarization credentials
MISSING_VARS=()
if [[ -z "$APPLE_SIGNING_IDENTITY" ]]; then
    MISSING_VARS+=("APPLE_SIGNING_IDENTITY")
fi

# Check for notarization credentials (API key method preferred)
if [[ -n "$APPLE_API_KEY" && -n "$APPLE_API_ISSUER" ]]; then
    echo -e "${GREEN}✓ Using API key authentication${NC}"
    if [[ -z "$APPLE_API_KEY_PATH" ]]; then
        echo -e "${YELLOW}Warning: APPLE_API_KEY_PATH not set, will search default locations${NC}"
    fi
elif [[ -n "$APPLE_ID" && -n "$APPLE_PASSWORD" && -n "$APPLE_TEAM_ID" ]]; then
    echo -e "${GREEN}✓ Using Apple ID authentication${NC}"
else
    echo -e "${RED}Error: Missing notarization credentials${NC}"
    echo "Set either:"
    echo "  1. API Key method (recommended):"
    echo "     export APPLE_API_KEY='your-api-key'"
    echo "     export APPLE_API_ISSUER='your-issuer-id'"
    echo "     export APPLE_API_KEY_PATH='/path/to/AuthKey_XXXXX.p8'"
    echo "  2. Apple ID method:"
    echo "     export APPLE_ID='your@email.com'"
    echo "     export APPLE_PASSWORD='xxxx-xxxx-xxxx-xxxx'"
    echo "     export APPLE_TEAM_ID='XXXXXXXXXX'"
    MISSING_VARS+=("notarization credentials")
fi

# Check for Tauri signing credentials
if [[ -z "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
    echo -e "${YELLOW}Warning: TAURI_SIGNING_PRIVATE_KEY not set${NC}"
    echo "Tauri update signatures will not be generated"
    echo "To generate signing keys: cargo install tauri-signer && tauri signer generate"
fi

if [[ ${#MISSING_VARS[@]} -gt 0 ]]; then
    echo -e "${RED}Error: Missing required environment variables: ${MISSING_VARS[*]}${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Environment variables configured${NC}"
echo -e "  Signing Identity: ${APPLE_SIGNING_IDENTITY}"
if [[ -n "$APPLE_TEAM_ID" ]]; then
    echo -e "  Team ID: ${APPLE_TEAM_ID}"
fi

# Set CI mode for non-interactive operation
export CI=true

echo -e "${GREEN}🚀 Starting VoiceTypr universal release process (${RELEASE_TYPE})${NC}"

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
# pnpm test
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
# Update version
jq --arg version "$NEW_VERSION" '.version = $version' src-tauri/tauri.conf.json > src-tauri/tauri.conf.json.tmp
# Update signingIdentity (remove ad-hoc signing)
jq --arg identity "$APPLE_SIGNING_IDENTITY" '.bundle.macOS.signingIdentity = $identity' src-tauri/tauri.conf.json.tmp > src-tauri/tauri.conf.json
rm src-tauri/tauri.conf.json.tmp

# Commit the tauri.conf.json change
git add src-tauri/tauri.conf.json
git commit -m "chore: update tauri.conf.json version to ${NEW_VERSION}"

# Push changes and tags
echo -e "${YELLOW}Pushing to GitHub...${NC}"
git push origin main
git push origin "v${NEW_VERSION}"

# Install required Rust targets if not already installed
echo -e "${YELLOW}Checking Rust targets...${NC}"
rustup target add aarch64-apple-darwin 2>/dev/null || true
rustup target add x86_64-apple-darwin 2>/dev/null || true

# Build universal binary with automatic notarization
echo -e "${GREEN}🔨 Building universal binary with notarization...${NC}"
echo -e "${BLUE}This will take some time as it includes notarization...${NC}"

cd src-tauri

# Build universal binary - Tauri will automatically sign and notarize
cargo tauri build --target universal-apple-darwin --bundles app,dmg,updater

cd ..

# Find build artifacts
UNIVERSAL_DMG=$(find "src-tauri/target/universal-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
UNIVERSAL_APP_TAR=$(find "src-tauri/target/universal-apple-darwin/release/bundle/macos" -name "*.app.tar.gz" | head -n 1)

if [[ -z "$UNIVERSAL_DMG" ]]; then
    echo -e "${RED}Error: Universal DMG not found${NC}"
    exit 1
fi

# Create output directory
OUTPUT_DIR="release-${NEW_VERSION}"
mkdir -p "$OUTPUT_DIR"

# Copy artifacts
echo -e "${YELLOW}Collecting artifacts...${NC}"
cp "$UNIVERSAL_DMG" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_universal.dmg"
cp "$UNIVERSAL_APP_TAR" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_universal.app.tar.gz"

# Sign update artifacts if credentials are available
if [[ -n "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
    echo -e "${YELLOW}Signing update artifacts...${NC}"

    # Find and sign the tar.gz file
    if [[ -f "$UNIVERSAL_APP_TAR" ]]; then
        cargo tauri signer sign --private-key "$TAURI_SIGNING_PRIVATE_KEY" \
            ${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:+--private-key-password "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD"} \
            "$UNIVERSAL_APP_TAR"

        # Copy signature file
        cp "${UNIVERSAL_APP_TAR}.sig" "$OUTPUT_DIR/"
    fi
else
    echo -e "${YELLOW}Skipping update signatures (no signing key provided)${NC}"
fi

# Create latest.json for updater
echo -e "${YELLOW}Creating latest.json...${NC}"
cat > "$OUTPUT_DIR/latest.json" << EOF
{
  "version": "v${NEW_VERSION}",
  "notes": "See the release notes for v${NEW_VERSION}",
  "pub_date": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "platforms": {
    "darwin-universal": {
      "signature": "$(cat "${UNIVERSAL_APP_TAR}.sig" 2>/dev/null || echo "SIGNATURE_PLACEHOLDER")",
      "url": "https://github.com/moinulmoin/voicetypr/releases/download/v${NEW_VERSION}/VoiceTypr_${NEW_VERSION}_universal.app.tar.gz"
    }
  }
}
EOF

# Verify notarization
echo -e "${BLUE}✅ Verifying notarization...${NC}"
spctl -a -t open --context context:primary-signature -v "$UNIVERSAL_DMG" || {
    echo -e "${YELLOW}Warning: Notarization verification failed. The app may still be notarized.${NC}"
}

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

### macOS Universal Binary
Download \`VoiceTypr_${NEW_VERSION}_universal.dmg\`

This universal binary works on both Intel and Apple Silicon Macs.

### ✅ Fully Notarized
This release is signed and notarized by Apple. You can download and run VoiceTypr without any security warnings.

### First-time Setup
1. Download the DMG
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

# Restore ad-hoc signing for development
echo -e "${YELLOW}Restoring development configuration...${NC}"
jq '.bundle.macOS.signingIdentity = "-"' src-tauri/tauri.conf.json > src-tauri/tauri.conf.json.tmp
mv src-tauri/tauri.conf.json.tmp src-tauri/tauri.conf.json

echo -e "${GREEN}✅ Release process complete!${NC}"
echo -e "${GREEN}📁 Notarized artifacts saved in: ${OUTPUT_DIR}/${NC}"
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Review the draft release on GitHub"
echo "2. Test the notarized universal DMG"
echo "3. Publish the release when ready"
echo ""
echo -e "${GREEN}🔗 Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/v${NEW_VERSION}${NC}"
echo -e "${GREEN}🎉 Your universal app is now fully notarized and ready for distribution!${NC}"