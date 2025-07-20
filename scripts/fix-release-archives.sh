#!/bin/bash

# Script to fix macOS tar.gz archives by removing AppleDouble files
# This prevents "failed to unpack ._" errors in Tauri updater

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}🔧 Fixing release archives to remove AppleDouble files${NC}"

# Function to repack a tar.gz without AppleDouble files
repack_archive() {
    local archive_path="$1"
    local arch="$2"
    
    if [[ ! -f "$archive_path" ]]; then
        echo -e "${YELLOW}Archive not found: $archive_path${NC}"
        return
    fi
    
    echo -e "${YELLOW}Processing $arch archive...${NC}"
    
    # Create temp directory
    TEMP_DIR=$(mktemp -d)
    
    # Extract original archive
    tar -xzf "$archive_path" -C "$TEMP_DIR"
    
    # Remove the original archive
    rm "$archive_path"
    
    # Repack with COPYFILE_DISABLE to prevent AppleDouble files
    cd "$TEMP_DIR"
    COPYFILE_DISABLE=1 tar -czf "$archive_path" --exclude='._*' --exclude='.DS_Store' *
    cd - > /dev/null
    
    # Clean up
    rm -rf "$TEMP_DIR"
    
    echo -e "${GREEN}✓ Fixed $arch archive${NC}"
}

# Find and fix x64 archive
X64_ARCHIVE=$(find src-tauri/target/x86_64-apple-darwin/release/bundle/macos -name "*.app.tar.gz" -not -name "*.sig" 2>/dev/null | head -1)
if [[ -n "$X64_ARCHIVE" ]]; then
    repack_archive "$X64_ARCHIVE" "x64"
fi

# Find and fix aarch64 archive
AARCH64_ARCHIVE=$(find src-tauri/target/aarch64-apple-darwin/release/bundle/macos -name "*.app.tar.gz" -not -name "*.sig" 2>/dev/null | head -1)
if [[ -n "$AARCH64_ARCHIVE" ]]; then
    repack_archive "$AARCH64_ARCHIVE" "aarch64"
fi

echo -e "${GREEN}✅ Archives fixed!${NC}"