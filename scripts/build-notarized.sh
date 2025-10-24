#!/bin/bash

# Build and notarize VoiceTypr without creating a release
# Usage: ./scripts/build-notarized.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Trap to ensure cleanup happens even on error
trap 'echo -e "${RED}Script failed! Check the error above.${NC}"' ERR

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
TAURI_KEY_PATH="$HOME/.tauri/voicetypr.key"
if [[ -z "$TAURI_SIGNING_PRIVATE_KEY" ]] && [[ -z "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]] && [[ ! -f "$TAURI_KEY_PATH" ]]; then
    echo -e "${YELLOW}Warning: Tauri signing key not configured${NC}"
    echo "Tauri update signatures will not be generated"
elif [[ -f "$TAURI_KEY_PATH" ]] && [[ -z "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
    export TAURI_SIGNING_PRIVATE_KEY_PATH="$TAURI_KEY_PATH"
    echo -e "${GREEN}✓ Tauri signing key found at $TAURI_KEY_PATH${NC}"
else
    echo -e "${GREEN}✓ Tauri signing configured${NC}"
fi

if [[ ${#MISSING_VARS[@]} -gt 0 ]]; then
    echo -e "${RED}Error: Missing required environment variables: ${MISSING_VARS[*]}${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Environment configured${NC}"
echo -e "  Signing Identity: ${APPLE_SIGNING_IDENTITY}"

# Get current version from package.json
VERSION=$(node -p "require('./package.json').version")
echo -e "${GREEN}Building version: ${VERSION}${NC}"

# Update signing identity in tauri.conf.json
echo -e "${YELLOW}Updating tauri.conf.json...${NC}"
jq --arg identity "$APPLE_SIGNING_IDENTITY" '.bundle.macOS.signingIdentity = $identity' src-tauri/tauri.conf.json > src-tauri/tauri.conf.json.tmp
mv src-tauri/tauri.conf.json.tmp src-tauri/tauri.conf.json

# Install required Rust targets if not already installed
echo -e "${YELLOW}Checking Rust targets...${NC}"
rustup target add aarch64-apple-darwin 2>/dev/null || true
rustup target add x86_64-apple-darwin 2>/dev/null || true

# Build universal binary with automatic notarization
echo -e "${GREEN}🔨 Building universal binary with notarization...${NC}"
echo -e "${BLUE}This will take some time as it includes notarization...${NC}"

cd src-tauri

# Clean previous builds
echo -e "${YELLOW}Cleaning previous builds...${NC}"
rm -rf target/universal-apple-darwin/release/bundle

# Build universal binary - Tauri will automatically sign and notarize
cargo tauri build --target universal-apple-darwin --bundles app,dmg --config tauri.macos.conf.json

cd ..

# Find build artifacts
UNIVERSAL_DMG=$(find "src-tauri/target/universal-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
UNIVERSAL_APP_DIR="src-tauri/target/universal-apple-darwin/release/bundle/macos"

# Create output directory
OUTPUT_DIR="notarized-build-${VERSION}"
mkdir -p "$OUTPUT_DIR"

# Create app.tar.gz for updater
echo -e "${YELLOW}Creating updater archive...${NC}"
if [[ -d "$UNIVERSAL_APP_DIR/voicetypr.app" ]]; then
    cd "$UNIVERSAL_APP_DIR"
    tar -czf "VoiceTypr_${VERSION}_universal.app.tar.gz" voicetypr.app
    cd - > /dev/null
    UNIVERSAL_APP_TAR="$UNIVERSAL_APP_DIR/VoiceTypr_${VERSION}_universal.app.tar.gz"
    
    # Copy to output
    cp "$UNIVERSAL_APP_TAR" "$OUTPUT_DIR/"
else
    echo -e "${RED}Error: App bundle not found${NC}"
    exit 1
fi

if [[ -z "$UNIVERSAL_DMG" ]]; then
    echo -e "${RED}Error: Universal DMG not found${NC}"
    exit 1
fi

# Copy DMG
echo -e "${YELLOW}Copying artifacts...${NC}"
cp "$UNIVERSAL_DMG" "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.dmg"

# Sign update artifacts if credentials are available
if [[ -n "$TAURI_SIGNING_PRIVATE_KEY" ]] || [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
    echo -e "${YELLOW}Signing update artifacts...${NC}"

    # Determine key path
    if [[ -f "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
        KEY_PATH="$TAURI_SIGNING_PRIVATE_KEY"
    elif [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
        KEY_PATH="$TAURI_SIGNING_PRIVATE_KEY_PATH"
    else
        TEMP_KEY=$(mktemp)
        echo "$TAURI_SIGNING_PRIVATE_KEY" > "$TEMP_KEY"
        KEY_PATH="$TEMP_KEY"
    fi

    # Sign the tar.gz
    if [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]]; then
        cargo tauri signer sign -f "$KEY_PATH" -p "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.app.tar.gz"
    else
        cargo tauri signer sign -f "$KEY_PATH" -p "" "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.app.tar.gz"
    fi

    # Clean up temp file if created
    if [[ -n "${TEMP_KEY:-}" ]] && [[ -f "$TEMP_KEY" ]]; then
        rm -f "$TEMP_KEY"
    fi

    if [[ -f "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.app.tar.gz.sig" ]]; then
        echo -e "${GREEN}✓ Signature created successfully${NC}"
    else
        echo -e "${RED}Warning: Signature file not created${NC}"
    fi
else
    echo -e "${YELLOW}Skipping update signatures (no signing key provided)${NC}"
fi

# Create latest.json for updater
echo -e "${YELLOW}Creating latest.json...${NC}"

# Get signature if it exists
if [[ -f "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.app.tar.gz.sig" ]]; then
    SIGNATURE=$(cat "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.app.tar.gz.sig" | tr -d '\n')
else
    SIGNATURE="SIGNATURE_PLACEHOLDER"
    echo -e "${YELLOW}Warning: No signature file found, using placeholder${NC}"
fi

# Create latest.json
printf '{
  "version": "v%s",
  "notes": "See the release notes for v%s",
  "pub_date": "%s",
  "platforms": {
    "darwin-universal": {
      "signature": "%s",
      "url": "https://github.com/moinulmoin/voicetypr/releases/download/v%s/VoiceTypr_%s_universal.app.tar.gz"
    }
  }
}\n' "$VERSION" "$VERSION" "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$SIGNATURE" "$VERSION" "$VERSION" > "$OUTPUT_DIR/latest.json"

# Verify notarization
echo -e "${BLUE}✅ Verifying notarization...${NC}"

# Check app bundle notarization
if [[ -d "$UNIVERSAL_APP_DIR/voicetypr.app" ]]; then
    echo -e "${YELLOW}Checking app bundle...${NC}"
    if spctl -a -t exec -vv "$UNIVERSAL_APP_DIR/voicetypr.app" 2>&1 | grep -q "accepted"; then
        echo -e "${GREEN}✓ App bundle is properly notarized${NC}"
    else
        echo -e "${RED}⚠️  App bundle notarization check failed${NC}"
        spctl -a -t exec -vv "$UNIVERSAL_APP_DIR/voicetypr.app" 2>&1
    fi
fi

# Check DMG notarization
echo -e "${YELLOW}Checking DMG...${NC}"
if spctl -a -t open --context context:primary-signature -v "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.dmg" 2>&1 | grep -q "accepted"; then
    echo -e "${GREEN}✓ DMG is properly notarized${NC}"
else
    echo -e "${YELLOW}⚠️  DMG notarization check inconclusive (this can be a false negative)${NC}"
fi

# Additional verification with stapler
echo -e "${YELLOW}Checking notarization ticket...${NC}"
if xcrun stapler validate "$OUTPUT_DIR/VoiceTypr_${VERSION}_universal.dmg" 2>&1 | grep -q "validated"; then
    echo -e "${GREEN}✓ Notarization ticket is valid${NC}"
else
    echo -e "${RED}⚠️  Notarization ticket check failed${NC}"
fi

# Restore ad-hoc signing for development
echo -e "${YELLOW}Restoring development configuration...${NC}"
jq '.bundle.macOS.signingIdentity = "-"' src-tauri/tauri.conf.json > src-tauri/tauri.conf.json.tmp
mv src-tauri/tauri.conf.json.tmp src-tauri/tauri.conf.json

echo -e "${GREEN}✅ Build complete!${NC}"
echo -e "${GREEN}📁 Notarized artifacts saved in: ${OUTPUT_DIR}/${NC}"
echo ""
echo -e "${BLUE}📦 Build artifacts:${NC}"
ls -lh "$OUTPUT_DIR" | grep -E '\.(dmg|tar\.gz|sig|json)$' | while read -r line; do
    echo "   $line"
done
echo ""
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Test the notarized DMG locally"
echo "2. Upload files to the existing GitHub release:"
echo "   - VoiceTypr_${VERSION}_universal.dmg"
echo "   - VoiceTypr_${VERSION}_universal.app.tar.gz"
echo "   - VoiceTypr_${VERSION}_universal.app.tar.gz.sig (if generated)"
echo "   - latest.json"
echo ""
echo -e "${GREEN}To upload to an existing release:${NC}"
echo "gh release upload v${VERSION} ${OUTPUT_DIR}/* --clobber"