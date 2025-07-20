#!/bin/bash

# Release script for VoiceTypr with Separate Architecture Binaries and Built-in Tauri Notarization
# Usage: ./scripts/release-separate.sh [patch|minor|major]

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

# Check for Tauri signing credentials - also check common path
TAURI_KEY_PATH="$HOME/.tauri/voicetypr.key"
if [[ -z "$TAURI_SIGNING_PRIVATE_KEY" ]] && [[ -z "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]] && [[ ! -f "$TAURI_KEY_PATH" ]]; then
    echo -e "${YELLOW}Warning: Tauri signing key not configured${NC}"
    echo "Tauri update signatures will not be generated"
    echo ""
    echo "To set up signing:"
    echo "1. Generate keys: cargo tauri signer generate -w ~/.tauri/voicetypr.key"
    echo "2. Set one of:"
    echo "   export TAURI_SIGNING_PRIVATE_KEY_PATH=\"$HOME/.tauri/voicetypr.key\""
    echo "   export TAURI_SIGNING_PRIVATE_KEY=\"\$(cat ~/.tauri/voicetypr.key)\""
    echo "3. If key has password: export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=\"your-password\""
    echo "4. Update tauri.conf.json with the public key from ~/.tauri/voicetypr.key.pub"
elif [[ -f "$TAURI_KEY_PATH" ]] && [[ -z "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
    # Auto-detect key at common location
    export TAURI_SIGNING_PRIVATE_KEY_PATH="$TAURI_KEY_PATH"
    echo -e "${GREEN}✓ Tauri signing key found at $TAURI_KEY_PATH${NC}"
else
    echo -e "${GREEN}✓ Tauri signing configured${NC}"
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

echo -e "${GREEN}🚀 Starting VoiceTypr separate architecture release process (${RELEASE_TYPE})${NC}"

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" != "main" ]]; then
    echo -e "${YELLOW}Warning: Not on main branch (currently on ${CURRENT_BRANCH})${NC}"
    # Only prompt if not in CI mode
    if [[ "$CI" != "true" ]]; then
        read -p "Continue anyway? (y/n) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
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
# pnpm test || {
#     echo -e "${RED}Frontend tests failed!${NC}"
#     exit 1
# }
cd src-tauri && cargo test && cd .. || {
    echo -e "${RED}Backend tests failed!${NC}"
    exit 1
}

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

# Create output directory
OUTPUT_DIR="release-${NEW_VERSION}"
mkdir -p "$OUTPUT_DIR"

# Function to sign update artifacts
sign_update_artifact() {
    local FILE_PATH="$1"
    
    if [[ -n "$TAURI_SIGNING_PRIVATE_KEY" ]] || [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
        echo -e "${YELLOW}Signing $(basename "$FILE_PATH")...${NC}"
        
        # Determine if we have a key path or key content
        if [[ -f "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
            # It's a file path
            KEY_PATH="$TAURI_SIGNING_PRIVATE_KEY"
        elif [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
            # Use the explicit path variable
            KEY_PATH="$TAURI_SIGNING_PRIVATE_KEY_PATH"
        else
            # It's key content - write to temp file
            TEMP_KEY=$(mktemp)
            echo "$TAURI_SIGNING_PRIVATE_KEY" > "$TEMP_KEY"
            KEY_PATH="$TEMP_KEY"
        fi
        
        # Sign with proper flags
        if [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]]; then
            cargo tauri signer sign -f "$KEY_PATH" -p "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" "$FILE_PATH"
        else
            # Try with empty password
            cargo tauri signer sign -f "$KEY_PATH" -p "" "$FILE_PATH"
        fi
        
        # Clean up temp file if created
        if [[ -n "${TEMP_KEY:-}" ]] && [[ -f "$TEMP_KEY" ]]; then
            rm -f "$TEMP_KEY"
        fi
        
        if [[ -f "${FILE_PATH}.sig" ]]; then
            echo -e "${GREEN}✓ Signature created for $(basename "$FILE_PATH")${NC}"
            return 0
        else
            echo -e "${RED}Warning: Signature file not created for $(basename "$FILE_PATH")${NC}"
            return 1
        fi
    else
        echo -e "${YELLOW}Skipping signature for $(basename "$FILE_PATH") (no signing key)${NC}"
        return 1
    fi
}

# Build x86_64 binary with automatic notarization
echo -e "${GREEN}🔨 Building x86_64 binary with notarization...${NC}"
echo -e "${BLUE}This will take some time as it includes notarization...${NC}"

cd src-tauri
cargo tauri build --target x86_64-apple-darwin --bundles app,dmg
cd ..

# Find x86_64 build artifacts
X86_DMG=$(find "src-tauri/target/x86_64-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
X86_APP_DIR="src-tauri/target/x86_64-apple-darwin/release/bundle/macos"

# Create app.tar.gz for x86_64
echo -e "${YELLOW}Creating x86_64 updater archive...${NC}"
if [[ -d "$X86_APP_DIR/voicetypr.app" ]]; then
    cd "$X86_APP_DIR"
    tar -czf "VoiceTypr_${NEW_VERSION}_x64.app.tar.gz" voicetypr.app
    cd - > /dev/null
    X86_APP_TAR="$X86_APP_DIR/VoiceTypr_${NEW_VERSION}_x64.app.tar.gz"
else
    echo -e "${RED}Error: x86_64 app bundle not found${NC}"
    exit 1
fi

if [[ -z "$X86_DMG" ]]; then
    echo -e "${RED}Error: x86_64 DMG not found${NC}"
    exit 1
fi

# Copy x86_64 artifacts
cp "$X86_DMG" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_x64.dmg"
cp "$X86_APP_TAR" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_x64.app.tar.gz"

# Sign x86_64 update artifact
sign_update_artifact "$X86_APP_TAR"
if [[ -f "${X86_APP_TAR}.sig" ]]; then
    cp "${X86_APP_TAR}.sig" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_x64.app.tar.gz.sig"
fi

# Build aarch64 binary with automatic notarization
echo -e "${GREEN}🔨 Building aarch64 binary with notarization...${NC}"
echo -e "${BLUE}This will take some time as it includes notarization...${NC}"

cd src-tauri
cargo tauri build --target aarch64-apple-darwin --bundles app,dmg
cd ..

# Find aarch64 build artifacts
AARCH64_DMG=$(find "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
AARCH64_APP_DIR="src-tauri/target/aarch64-apple-darwin/release/bundle/macos"

# Create app.tar.gz for aarch64
echo -e "${YELLOW}Creating aarch64 updater archive...${NC}"
if [[ -d "$AARCH64_APP_DIR/voicetypr.app" ]]; then
    cd "$AARCH64_APP_DIR"
    tar -czf "VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz" voicetypr.app
    cd - > /dev/null
    AARCH64_APP_TAR="$AARCH64_APP_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz"
else
    echo -e "${RED}Error: aarch64 app bundle not found${NC}"
    exit 1
fi

if [[ -z "$AARCH64_DMG" ]]; then
    echo -e "${RED}Error: aarch64 DMG not found${NC}"
    exit 1
fi

# Copy aarch64 artifacts
cp "$AARCH64_DMG" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.dmg"
cp "$AARCH64_APP_TAR" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz"

# Sign aarch64 update artifact
sign_update_artifact "$AARCH64_APP_TAR"
if [[ -f "${AARCH64_APP_TAR}.sig" ]]; then
    cp "${AARCH64_APP_TAR}.sig" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz.sig"
fi

# Create latest.json for updater with both architectures
echo -e "${YELLOW}Creating latest.json...${NC}"

# Get signatures from the sig files if they exist
if [[ -f "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_x64.app.tar.gz.sig" ]]; then
    X86_SIGNATURE=$(cat "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_x64.app.tar.gz.sig" | tr -d '\n')
else
    X86_SIGNATURE="SIGNATURE_PLACEHOLDER"
    echo -e "${YELLOW}Warning: No x86_64 signature file found${NC}"
fi

if [[ -f "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz.sig" ]]; then
    AARCH64_SIGNATURE=$(cat "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz.sig" | tr -d '\n')
else
    AARCH64_SIGNATURE="SIGNATURE_PLACEHOLDER"
    echo -e "${YELLOW}Warning: No aarch64 signature file found${NC}"
fi

# Create latest.json with both platforms
printf '{
  "version": "v%s",
  "notes": "See the release notes for v%s",
  "pub_date": "%s",
  "platforms": {
    "darwin-x86_64": {
      "signature": "%s",
      "url": "https://github.com/moinulmoin/voicetypr/releases/download/v%s/VoiceTypr_%s_x64.app.tar.gz"
    },
    "darwin-aarch64": {
      "signature": "%s",
      "url": "https://github.com/moinulmoin/voicetypr/releases/download/v%s/VoiceTypr_%s_aarch64.app.tar.gz"
    }
  }
}\n' "$NEW_VERSION" "$NEW_VERSION" "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$X86_SIGNATURE" "$NEW_VERSION" "$NEW_VERSION" "$AARCH64_SIGNATURE" "$NEW_VERSION" "$NEW_VERSION" > "$OUTPUT_DIR/latest.json"

# Verify notarization
echo -e "${BLUE}✅ Verifying notarization...${NC}"

# Check x86_64 app bundle
if [[ -d "$X86_APP_DIR/voicetypr.app" ]]; then
    spctl -a -t exec -vv "$X86_APP_DIR/voicetypr.app" 2>&1 | grep -q "accepted" && {
        echo -e "${GREEN}✓ x86_64 app bundle is properly notarized${NC}"
    } || {
        echo -e "${YELLOW}Warning: x86_64 app bundle notarization check failed${NC}"
    }
fi

# Check aarch64 app bundle
if [[ -d "$AARCH64_APP_DIR/voicetypr.app" ]]; then
    spctl -a -t exec -vv "$AARCH64_APP_DIR/voicetypr.app" 2>&1 | grep -q "accepted" && {
        echo -e "${GREEN}✓ aarch64 app bundle is properly notarized${NC}"
    } || {
        echo -e "${YELLOW}Warning: aarch64 app bundle notarization check failed${NC}"
    }
fi

# Create GitHub release using gh CLI
echo -e "${YELLOW}Creating GitHub release draft...${NC}"
if command -v gh &> /dev/null; then
    # Extract changelog content for this version
    if [[ -f "CHANGELOG.md" ]]; then
        # Look for the version header with square brackets and get content until next version or end
        # Handle both # [x.x.x] and ## [x.x.x] formats
        CHANGELOG_CONTENT=$(awk -v ver="${NEW_VERSION}" '
            $0 ~ "^#+ \\[" ver "\\]" {flag=1; next}
            /^#+ \[[0-9]+\.[0-9]+\.[0-9]+\]/ && flag {exit}
            flag {print}
        ' CHANGELOG.md)
        
        if [[ -z "$CHANGELOG_CONTENT" ]]; then
            CHANGELOG_CONTENT="See the full changelog at https://github.com/moinulmoin/voicetypr/blob/main/CHANGELOG.md"
        fi
    else
        CHANGELOG_CONTENT="Initial release"
    fi
    
    gh release create "v${NEW_VERSION}" \
        --draft \
        --title "VoiceTypr v${NEW_VERSION}" \
        --notes "$CHANGELOG_CONTENT"
    
    echo -e "${GREEN}✅ Draft release created!${NC}"
    echo -e "${YELLOW}Uploading artifacts...${NC}"
    
    # Upload all artifacts
    for file in "$OUTPUT_DIR"/*; do
        echo -e "  Uploading: $(basename "$file")"
        gh release upload "v${NEW_VERSION}" "$file" --clobber
    done
    
    echo -e "${GREEN}✓ All artifacts uploaded successfully${NC}"
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
echo ""
echo -e "${BLUE}📦 Release artifacts:${NC}"
ls -lh "$OUTPUT_DIR" | grep -E '\.(dmg|tar\.gz|sig|json)$' | while read -r line; do
    echo "   $line"
done
echo ""
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Review the draft release on GitHub"
echo "2. Test the notarized DMGs for both architectures"
echo "3. Verify auto-updater works with the new signatures"
echo "4. Publish the release when ready"
echo ""
echo -e "${GREEN}🔗 Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/v${NEW_VERSION}${NC}"
echo -e "${GREEN}🎉 Your separate architecture apps are now fully notarized and ready for distribution!${NC}"