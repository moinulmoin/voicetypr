#!/bin/bash

# Release script for VoiceTypr with Separate Architecture Binaries and Built-in Tauri Notarization
# Usage: ./scripts/release-separate.sh [patch|minor|major]

set -euo pipefail

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

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}Error: Required command not found: $cmd${NC}"
        exit 1
    fi
}

require_file() {
    local path="$1"
    if [[ ! -f "$path" ]]; then
        echo -e "${RED}Error: Required file not found: $path${NC}"
        exit 1
    fi
}

# Load .env file FIRST if it exists
if [ -f .env ]; then
    echo -e "${YELLOW}Loading environment variables from .env...${NC}"
    set -a
    source .env
    set +a
    echo -e "${GREEN}✓ Environment variables loaded${NC}"
fi

# Check for required environment variables
echo -e "${YELLOW}Checking environment variables...${NC}"

# Check Apple signing/notarization credentials
MISSING_VARS=()
if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    MISSING_VARS+=("APPLE_SIGNING_IDENTITY")
fi

# Check for notarization credentials (API key method preferred)
if [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
    echo -e "${GREEN}✓ Using API key authentication${NC}"
    if [[ -z "${APPLE_API_KEY_PATH:-}" ]]; then
        # Try common locations for AuthKey_XXXX.p8
        CANDIDATES=(
            "$HOME/.private_keys/AuthKey_${APPLE_API_KEY}.p8"
            "$HOME/private_keys/AuthKey_${APPLE_API_KEY}.p8"
            "$HOME/Downloads/AuthKey_${APPLE_API_KEY}.p8"
            "$PWD/AuthKey_${APPLE_API_KEY}.p8"
        )
        for candidate in "${CANDIDATES[@]}"; do
            if [[ -f "$candidate" ]]; then
                export APPLE_API_KEY_PATH="$candidate"
                echo -e "${GREEN}✓ Found APPLE_API_KEY_PATH at $APPLE_API_KEY_PATH${NC}"
                break
            fi
        done
    fi

    if [[ -z "${APPLE_API_KEY_PATH:-}" ]]; then
        echo -e "${RED}Error: APPLE_API_KEY_PATH not set and AuthKey file not found${NC}"
        MISSING_VARS+=("APPLE_API_KEY_PATH")
    fi
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
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
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] && [[ -z "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]] && [[ ! -f "$TAURI_KEY_PATH" ]]; then
    echo -e "${RED}Error: Tauri signing key not configured${NC}"
    echo "Update signatures are required for auto-updates. Configure one of:"
    echo "1. Generate keys: cargo tauri signer generate -w ~/.tauri/voicetypr.key"
    echo "2. Set one of:"
    echo "   export TAURI_SIGNING_PRIVATE_KEY_PATH=\"$HOME/.tauri/voicetypr.key\""
    echo "   export TAURI_SIGNING_PRIVATE_KEY=\"\$(cat ~/.tauri/voicetypr.key)\""
    echo "3. If key has password: export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=\"your-password\""
    MISSING_VARS+=("TAURI signing key")
elif [[ -f "$TAURI_KEY_PATH" ]] && [[ -z "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
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
if [[ -n "${APPLE_TEAM_ID:-}" ]]; then
    echo -e "  Team ID: ${APPLE_TEAM_ID}"
fi

# Set CI mode for non-interactive operation
export CI=true

echo -e "${GREEN}🚀 Starting VoiceTypr separate architecture release process (${RELEASE_TYPE})${NC}"

require_cmd git
require_cmd pnpm
require_cmd jq
require_cmd cargo
require_cmd gh
require_file package.json
require_file src-tauri/Cargo.toml

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" != "main" ]]; then
    echo -e "${RED}Error: Must run releases from main branch (currently on ${CURRENT_BRANCH})${NC}"
    exit 1
fi

# Check for uncommitted changes
if [[ -n $(git status -s) ]]; then
    echo -e "${RED}Error: You have uncommitted changes${NC}"
    git status -s
    exit 1
fi

# Pull latest changes
echo -e "${YELLOW}Pulling latest changes...${NC}"
git pull --ff-only origin main

# Run tests first
echo -e "${YELLOW}Running tests...${NC}"
pnpm test:backend

# Get current version
CURRENT_VERSION=$(node -p "require('./package.json').version")
echo -e "${GREEN}Current version: ${CURRENT_VERSION}${NC}"

# Use release-it to handle version bump, changelog, tag, and draft release
echo -e "${YELLOW}Running release-it...${NC}"
# Ensure GITHUB_TOKEN is set for release-it (get from gh CLI if not set)
if [[ -z "${GITHUB_TOKEN:-}" ]]; then
    echo -e "${YELLOW}GITHUB_TOKEN not in env, getting from gh CLI...${NC}"
    export GITHUB_TOKEN=$(gh auth token)
else
    echo -e "${GREEN}✓ GITHUB_TOKEN found (${GITHUB_TOKEN:0:10}...)${NC}"
fi
export GITHUB_TOKEN
pnpm -s release "$RELEASE_TYPE" --ci

# Get new version
NEW_VERSION=$(node -p "require('./package.json').version")
echo -e "${GREEN}New version: ${NEW_VERSION}${NC}"

# Install required Rust targets if not already installed
echo -e "${YELLOW}Checking Rust targets...${NC}"
rustup target add aarch64-apple-darwin 2>/dev/null || true

# Create output directory
OUTPUT_DIR="release-${NEW_VERSION}"
mkdir -p "$OUTPUT_DIR"

# Function to sign update artifacts
sign_update_artifact() {
    local FILE_PATH="$1"
    
    if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] || [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
        echo -e "${YELLOW}Signing $(basename "$FILE_PATH")...${NC}"
        
        # Determine if we have a key path or key content
        if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
            KEY_PATH="$TAURI_SIGNING_PRIVATE_KEY_PATH"
        else
            # It's key content - write to temp file
            TEMP_KEY=$(mktemp)
            echo "${TAURI_SIGNING_PRIVATE_KEY}" > "$TEMP_KEY"
            KEY_PATH="$TEMP_KEY"
        fi
        
        # Sign with proper flags
        if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]]; then
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
        echo -e "${RED}Error: Missing Tauri signing key; cannot sign update artifacts${NC}"
        return 1
    fi
}

# Build Parakeet sidecar (Apple Silicon, arm64) and prepare dist symlink
build_parakeet_sidecar() {
    echo -e "${YELLOW}Building Parakeet sidecar (arm64)...${NC}"
    local SIDE_DIR="sidecar/parakeet-swift"
    if [[ ! -d "$SIDE_DIR" ]]; then
        echo -e "${YELLOW}Parakeet sidecar directory not found at $SIDE_DIR; skipping sidecar build${NC}"
        return 0
    fi

    if ! command -v swift >/dev/null 2>&1; then
        echo -e "${RED}Swift toolchain not found. Install Xcode Command Line Tools to build sidecar.${NC}"
        exit 1
    fi

    pushd "$SIDE_DIR" > /dev/null

    # Build for Apple Silicon
    swift build -c release --arch arm64
    # Determine binary output path
    BIN_DIR=$(swift build -c release --arch arm64 --show-bin-path 2>/dev/null || echo ".build/arm64-apple-macosx/release")
    SRC_BIN_NAME="ParakeetSidecar"
    SRC_BIN_PATH="$BIN_DIR/$SRC_BIN_NAME"

    if [[ ! -f "$SRC_BIN_PATH" ]]; then
        echo -e "${RED}Error: Built sidecar not found at $SRC_BIN_PATH${NC}"
        popd > /dev/null
        exit 1
    fi

    mkdir -p dist
    cp "$SRC_BIN_PATH" "dist/parakeet-sidecar-aarch64-apple-darwin"
    chmod +x "dist/parakeet-sidecar-aarch64-apple-darwin"
    ln -sfn "parakeet-sidecar-aarch64-apple-darwin" "dist/parakeet-sidecar"

    echo -e "${GREEN}✓ Parakeet sidecar built and prepared at $SIDE_DIR/dist${NC}"
    popd > /dev/null
}

# Ensure ffmpeg/ffprobe sidecar binaries exist before packaging
ensure_ffmpeg_sidecar() {
    echo -e "${YELLOW}Ensuring ffmpeg sidecar binaries...${NC}"
    pnpm run sidecar:ensure-ffmpeg

    local DIST_DIR="sidecar/ffmpeg/dist"
    if [[ ! -d "$DIST_DIR" ]]; then
        echo -e "${RED}Error: sidecar directory missing at $DIST_DIR${NC}"
        exit 1
    fi

    local REQUIRED_BINARIES=()
    case "$(uname -s)" in
        Darwin|Linux)
            REQUIRED_BINARIES=("ffmpeg" "ffprobe")
            ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            REQUIRED_BINARIES=("ffmpeg.exe" "ffprobe.exe")
            ;;
        *)
            REQUIRED_BINARIES=("ffmpeg" "ffprobe")
            ;;
    esac

    local missing=()
    for bin in "${REQUIRED_BINARIES[@]}"; do
        if [[ ! -f "$DIST_DIR/$bin" ]]; then
            missing+=("$bin")
        fi
    done

    if [[ ${#missing[@]} -gt 0 ]]; then
        echo -e "${RED}Error: Missing ffmpeg sidecar binaries: ${missing[*]}${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ ffmpeg sidecar binaries present${NC}"
}
# Intel (x86_64) build removed – Apple Silicon only

## Ensure sidecar is built and available for bundling
build_parakeet_sidecar
ensure_ffmpeg_sidecar

# Build aarch64 binary with automatic notarization
echo -e "${GREEN}🔨 Building aarch64 binary with notarization...${NC}"
echo -e "${BLUE}This will take some time as it includes notarization...${NC}"

# Build with an override config for signing identity (version comes from Cargo.toml)
TAURI_CONFIG_OVERRIDE=$(jq -nc --arg identity "$APPLE_SIGNING_IDENTITY" '{bundle:{macOS:{signingIdentity:$identity}}}')
pnpm -s tauri build --target aarch64-apple-darwin --bundles app,dmg --config "$TAURI_CONFIG_OVERRIDE" --ci

# Find aarch64 build artifacts
AARCH64_DMG=$(find "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
AARCH64_APP_DIR="src-tauri/target/aarch64-apple-darwin/release/bundle/macos"

# Create app.tar.gz for aarch64
echo -e "${YELLOW}Creating aarch64 updater archive...${NC}"
APP_BUNDLE_PATH=$(find "$AARCH64_APP_DIR" -maxdepth 1 -name "*.app" | head -n 1)
if [[ -n "${APP_BUNDLE_PATH}" && -d "$APP_BUNDLE_PATH" ]]; then
    cd "$AARCH64_APP_DIR"
    APP_BUNDLE_NAME=$(basename "$APP_BUNDLE_PATH")
    COPYFILE_DISABLE=1 tar -czf "VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz" --exclude='._*' --exclude='.DS_Store' "$APP_BUNDLE_NAME"
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
sign_update_artifact "$AARCH64_APP_TAR" || {
    echo -e "${RED}Error: Failed to sign update artifact${NC}"
    exit 1
}
if [[ -f "${AARCH64_APP_TAR}.sig" ]]; then
    cp "${AARCH64_APP_TAR}.sig" "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz.sig"
fi

# Create latest.json for updater (Apple Silicon only)
echo -e "${YELLOW}Creating latest.json...${NC}"

# Get aarch64 signature from the sig file if it exists
if [[ -f "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz.sig" ]]; then
    AARCH64_SIGNATURE=$(cat "$OUTPUT_DIR/VoiceTypr_${NEW_VERSION}_aarch64.app.tar.gz.sig" | tr -d '\n')
else
    echo -e "${RED}Error: No aarch64 signature file found${NC}"
    exit 1
fi

# Create latest.json with Apple Silicon only
printf '{
  "version": "v%s",
  "notes": "See the release notes for v%s",
  "pub_date": "%s",
  "platforms": {
    "darwin-aarch64": {
      "signature": "%s",
      "url": "https://github.com/moinulmoin/voicetypr/releases/download/v%s/VoiceTypr_%s_aarch64.app.tar.gz"
    }
  }
}\n' "$NEW_VERSION" "$NEW_VERSION" "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$AARCH64_SIGNATURE" "$NEW_VERSION" "$NEW_VERSION" > "$OUTPUT_DIR/latest.json"

# Verify notarization
echo -e "${BLUE}✅ Verifying notarization...${NC}"

# Check aarch64 app bundle
if [[ -n "${APP_BUNDLE_PATH}" && -d "$APP_BUNDLE_PATH" ]]; then
    spctl -a -t exec -vv "$APP_BUNDLE_PATH" 2>&1 | grep -q "accepted" && {
        echo -e "${GREEN}✓ aarch64 app bundle is properly notarized${NC}"
    } || {
        echo -e "${YELLOW}Warning: aarch64 app bundle notarization check failed${NC}"
    }
fi

# Upload artifacts to the draft GitHub release created by release-it
echo -e "${YELLOW}Uploading artifacts to GitHub release v${NEW_VERSION}...${NC}"
gh release view "v${NEW_VERSION}" >/dev/null
for file in "$OUTPUT_DIR"/*; do
    echo -e "  Uploading: $(basename "$file")"
    gh release upload "v${NEW_VERSION}" "$file" --clobber
done
echo -e "${GREEN}✓ All artifacts uploaded successfully${NC}"

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
echo "2. Test the notarized DMG (Apple Silicon)"
echo "3. Verify auto-updater works with the new signatures"
echo "4. Publish the release when ready"
echo ""
echo -e "${GREEN}🔗 Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/v${NEW_VERSION}${NC}"
echo -e "${GREEN}🎉 Your Apple Silicon app is now fully notarized and ready for distribution!${NC}"
