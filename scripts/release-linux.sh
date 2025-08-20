#!/bin/bash

# Linux Release Script for VoiceTypr
# Usage: ./scripts/release-linux.sh [version]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get version from package.json if not provided
VERSION=${1:-$(node -p "require('./package.json').version")}
RELEASE_TAG="v${VERSION}"
OUTPUT_DIR="release-linux-${VERSION}"

echo -e "${GREEN}🚀 VoiceTypr Linux Release v${VERSION}${NC}"

# Check if we're in the right directory
if [[ ! -f "package.json" ]] || [[ ! -d "src-tauri" ]]; then
    echo -e "${RED}Error: Must run from project root${NC}"
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"
echo -e "${GREEN}✓ Output directory created: ${OUTPUT_DIR}${NC}"

# Function to sign update artifacts
sign_update_artifact() {
    local FILE_PATH="$1"
    
    # Check for Tauri signing key in common locations
    TAURI_KEY_PATH="$HOME/.tauri/voicetypr.key"
    
    if [[ -f "$TAURI_KEY_PATH" ]] || [[ -n "$TAURI_SIGNING_PRIVATE_KEY" ]] || [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
        echo -e "${YELLOW}Signing $(basename "$FILE_PATH")...${NC}"
        
        # Determine key source
        if [[ -f "$TAURI_KEY_PATH" ]] && [[ -z "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
            export TAURI_SIGNING_PRIVATE_KEY_PATH="$TAURI_KEY_PATH"
        fi
        
        # Sign with cargo tauri signer
        if [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
            KEY_PATH="$TAURI_SIGNING_PRIVATE_KEY_PATH"
        elif [[ -n "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
            # Write key content to temp file
            TEMP_KEY=$(mktemp)
            echo "$TAURI_SIGNING_PRIVATE_KEY" > "$TEMP_KEY"
            KEY_PATH="$TEMP_KEY"
        fi
        
        # Sign the file
        if [[ -n "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]]; then
            cargo tauri signer sign -f "$KEY_PATH" -p "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" "$FILE_PATH"
        else
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
            echo -e "${YELLOW}Warning: Signature file not created for $(basename "$FILE_PATH")${NC}"
            return 1
        fi
    else
        echo -e "${YELLOW}Skipping signature for $(basename "$FILE_PATH") (no signing key)${NC}"
        return 1
    fi
}

# Check for required dependencies
echo -e "${YELLOW}Checking Linux build dependencies...${NC}"

# Function to check if a package is installed (works on multiple distros)
check_dependency() {
    local pkg=$1
    
    # Try dpkg first (Debian/Ubuntu)
    if command -v dpkg &> /dev/null; then
        if dpkg -l | grep -q "^ii.*$pkg"; then
            return 0
        fi
    fi
    
    # Try rpm (Fedora/RHEL)
    if command -v rpm &> /dev/null; then
        if rpm -qa | grep -q "$pkg"; then
            return 0
        fi
    fi
    
    # Try pacman (Arch)
    if command -v pacman &> /dev/null; then
        if pacman -Q "$pkg" &> /dev/null; then
            return 0
        fi
    fi
    
    return 1
}

# Check for essential packages
MISSING_DEPS=()
REQUIRED_PACKAGES=(
    "libwebkit2gtk-4.1-dev"
    "build-essential"
    "libssl-dev"
    "libgtk-3-dev"
    "libayatana-appindicator3-dev"
    "librsvg2-dev"
    "patchelf"
    "libfuse2"
    "file"
)

for pkg in "${REQUIRED_PACKAGES[@]}"; do
    # Simplify package names for checking (remove -dev suffixes for runtime check)
    check_pkg=$(echo "$pkg" | sed 's/-dev$//')
    if ! check_dependency "$check_pkg" && ! check_dependency "$pkg"; then
        MISSING_DEPS+=("$pkg")
    fi
done

if [[ ${#MISSING_DEPS[@]} -gt 0 ]]; then
    echo -e "${YELLOW}Warning: Some dependencies might be missing:${NC}"
    echo -e "${YELLOW}${MISSING_DEPS[*]}${NC}"
    echo -e "${BLUE}Install them with:${NC}"
    
    if command -v apt &> /dev/null; then
        echo "  sudo apt update && sudo apt install ${MISSING_DEPS[*]}"
    elif command -v dnf &> /dev/null; then
        echo "  sudo dnf install ${MISSING_DEPS[*]}"
    elif command -v pacman &> /dev/null; then
        echo "  sudo pacman -S ${MISSING_DEPS[*]}"
    fi
    
    echo -e "${YELLOW}Continue anyway? (y/n)${NC}"
    read -r response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

echo -e "${GREEN}✓ Dependencies check complete${NC}"

# Build the AppImage
echo -e "${GREEN}🔨 Building AppImage for Linux...${NC}"

# Clean previous builds
echo -e "${YELLOW}Cleaning previous builds...${NC}"
rm -rf src-tauri/target/release/bundle/appimage

# Build with Tauri
echo -e "${YELLOW}Building with Tauri...${NC}"
cd src-tauri
cargo tauri build --bundles appimage
cd ..

# Find the AppImage
APPIMAGE=$(find "src-tauri/target/release/bundle/appimage" -name "*.AppImage" | head -n 1)

if [[ -z "$APPIMAGE" ]]; then
    echo -e "${RED}Error: AppImage not found${NC}"
    exit 1
fi

echo -e "${GREEN}✓ AppImage built successfully${NC}"

# Copy AppImage to output directory
APPIMAGE_NAME="VoiceTypr_${VERSION}_amd64.AppImage"
cp "$APPIMAGE" "$OUTPUT_DIR/$APPIMAGE_NAME"
echo -e "${GREEN}✓ AppImage copied to: $OUTPUT_DIR/$APPIMAGE_NAME${NC}"

# Sign the AppImage for updates
sign_update_artifact "$OUTPUT_DIR/$APPIMAGE_NAME"
if [[ -f "$OUTPUT_DIR/${APPIMAGE_NAME}.sig" ]]; then
    SIGNATURE=$(cat "$OUTPUT_DIR/${APPIMAGE_NAME}.sig" | tr -d '\n')
else
    SIGNATURE=""
    echo -e "${YELLOW}Warning: No signature generated for AppImage${NC}"
fi

# Update or create latest.json
echo -e "${YELLOW}Updating latest.json...${NC}"

LATEST_JSON_PATH="$OUTPUT_DIR/latest.json"

# Try to download existing latest.json from GitHub release
if command -v gh &> /dev/null; then
    echo -e "${YELLOW}Checking for existing latest.json...${NC}"
    
    # Try to download from release or draft
    if gh release download "$RELEASE_TAG" -p "latest.json" -D "$OUTPUT_DIR" --clobber 2>/dev/null; then
        echo -e "${GREEN}✓ Downloaded existing latest.json${NC}"
    else
        echo -e "${YELLOW}No existing latest.json found, will create new${NC}"
    fi
fi

if [[ -f "$LATEST_JSON_PATH" ]]; then
    # Update existing latest.json
    echo -e "${YELLOW}Updating existing latest.json with Linux platform...${NC}"
    
    # Use jq to update the JSON
    if command -v jq &> /dev/null; then
        jq --arg sig "$SIGNATURE" \
           --arg url "https://github.com/moinulmoin/voicetypr/releases/download/${RELEASE_TAG}/${APPIMAGE_NAME}" \
           '.platforms["linux-x86_64"] = {"signature": $sig, "url": $url}' \
           "$LATEST_JSON_PATH" > "$LATEST_JSON_PATH.tmp"
        mv "$LATEST_JSON_PATH.tmp" "$LATEST_JSON_PATH"
    else
        echo -e "${YELLOW}jq not found, manually creating latest.json${NC}"
        # Fallback: create complete latest.json
        cat > "$LATEST_JSON_PATH" << EOF
{
  "version": "${RELEASE_TAG}",
  "notes": "See the release notes for ${RELEASE_TAG}",
  "pub_date": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "platforms": {
    "linux-x86_64": {
      "signature": "${SIGNATURE}",
      "url": "https://github.com/moinulmoin/voicetypr/releases/download/${RELEASE_TAG}/${APPIMAGE_NAME}"
    }
  }
}
EOF
    fi
else
    # Create new latest.json for Linux
    echo -e "${YELLOW}Creating new latest.json for Linux...${NC}"
    cat > "$LATEST_JSON_PATH" << EOF
{
  "version": "${RELEASE_TAG}",
  "notes": "See the release notes for ${RELEASE_TAG}",
  "pub_date": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "platforms": {
    "linux-x86_64": {
      "signature": "${SIGNATURE}",
      "url": "https://github.com/moinulmoin/voicetypr/releases/download/${RELEASE_TAG}/${APPIMAGE_NAME}"
    }
  }
}
EOF
fi

echo -e "${GREEN}✓ latest.json updated${NC}"

# Upload to GitHub if gh CLI is available
if command -v gh &> /dev/null; then
    echo -e "${YELLOW}Uploading to GitHub release...${NC}"
    
    # Check if release exists
    if gh release view "$RELEASE_TAG" &> /dev/null; then
        echo -e "${GREEN}✓ Release $RELEASE_TAG found${NC}"
        
        # Upload AppImage
        echo -e "${YELLOW}Uploading AppImage...${NC}"
        gh release upload "$RELEASE_TAG" "$OUTPUT_DIR/$APPIMAGE_NAME" --clobber || {
            echo -e "${YELLOW}Warning: Failed to upload AppImage${NC}"
        }
        
        # Upload signature if exists
        if [[ -f "$OUTPUT_DIR/${APPIMAGE_NAME}.sig" ]]; then
            echo -e "${YELLOW}Uploading signature...${NC}"
            gh release upload "$RELEASE_TAG" "$OUTPUT_DIR/${APPIMAGE_NAME}.sig" --clobber || {
                echo -e "${YELLOW}Warning: Failed to upload signature${NC}"
            }
        fi
        
        # Upload latest.json
        echo -e "${YELLOW}Uploading latest.json...${NC}"
        gh release upload "$RELEASE_TAG" "$OUTPUT_DIR/latest.json" --clobber || {
            echo -e "${YELLOW}Warning: Failed to upload latest.json${NC}"
        }
        
        echo -e "${GREEN}✓ All artifacts uploaded successfully${NC}"
    else
        echo -e "${YELLOW}Release $RELEASE_TAG not found. Create it first with:${NC}"
        echo "  gh release create $RELEASE_TAG --draft --title \"VoiceTypr ${RELEASE_TAG}\""
    fi
else
    echo -e "${YELLOW}GitHub CLI not found. Install with:${NC}"
    echo "  curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo tee /usr/share/keyrings/githubcli-archive-keyring.gpg"
    echo "  echo \"deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main\" | sudo tee /etc/apt/sources.list.d/github-cli.list"
    echo "  sudo apt update && sudo apt install gh"
fi

# Summary
echo -e "${GREEN}✅ Linux release build complete!${NC}"
echo -e "${GREEN}📁 Artifacts saved in: ${OUTPUT_DIR}/${NC}"
echo ""
echo -e "${BLUE}📦 Release artifacts:${NC}"
ls -lh "$OUTPUT_DIR" | grep -E '\.(AppImage|sig|json)$' | while read -r line; do
    echo "   $line"
done
echo ""
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Test the AppImage on different Linux distributions"
echo "2. Verify it works on systems without development libraries"
echo "3. Test auto-updater functionality"
echo "4. Upload to GitHub release when ready"
echo ""
echo -e "${GREEN}🔗 Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/${RELEASE_TAG}${NC}"
echo -e "${GREEN}🎉 Your Linux AppImage is ready for distribution!${NC}"