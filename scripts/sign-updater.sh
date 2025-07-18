#!/bin/bash
set -e

# Check if required tools are installed
if ! command -v minisign &> /dev/null; then
    echo "minisign is not installed. Installing..."
    brew install minisign
fi

# Configuration
KEY_PATH="$HOME/.tauri/voicetypr.key"
RELEASE_DIR="release-1.0.0"
TAR_FILE="VoiceTypr_1.0.0_universal.app.tar.gz"

# Check if key exists
if [ ! -f "$KEY_PATH" ]; then
    echo "Error: Private key not found at $KEY_PATH"
    exit 1
fi

# Check if tar file exists
if [ ! -f "$RELEASE_DIR/$TAR_FILE" ]; then
    echo "Error: $TAR_FILE not found in $RELEASE_DIR"
    exit 1
fi

# Sign the file using minisign
echo "Signing $TAR_FILE with minisign..."
cd "$RELEASE_DIR"

# Use minisign to create signature
# -S flag for signing, -s for secret key file, -x for output signature extension
minisign -S -s "$KEY_PATH" -x "${TAR_FILE}.sig" -m "$TAR_FILE"

# Verify the signature was created
if [ -f "${TAR_FILE}.sig" ]; then
    echo "✅ Signature created successfully: ${TAR_FILE}.sig"
    
    # Read and display the signature (base64 encoded)
    echo "Signature content:"
    cat "${TAR_FILE}.sig"
else
    echo "❌ Failed to create signature"
    exit 1
fi

cd -