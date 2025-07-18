#!/bin/bash
set -e

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

# Read the private key content
PRIVATE_KEY=$(cat "$KEY_PATH")

# Export the key for cargo tauri signer
export TAURI_SIGNING_PRIVATE_KEY="$PRIVATE_KEY"

# Alternative: Try with password if the key is encrypted
# export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="your-password-here"

echo "Signing $TAR_FILE with cargo tauri signer..."
cd "$RELEASE_DIR"

# Try signing with cargo tauri
if cargo tauri signer sign "$TAR_FILE"; then
    echo "✅ Signature created successfully"
    
    # The signature should be in the same directory with .sig extension
    if [ -f "${TAR_FILE}.sig" ]; then
        echo "Signature file: ${TAR_FILE}.sig"
        echo "Signature content:"
        cat "${TAR_FILE}.sig"
    fi
else
    echo "❌ cargo tauri signer failed"
    echo ""
    echo "Trying alternative method with base64 encoded key..."
    
    # Sometimes the key needs to be base64 encoded
    ENCODED_KEY=$(echo -n "$PRIVATE_KEY" | base64)
    export TAURI_SIGNING_PRIVATE_KEY="$ENCODED_KEY"
    
    if cargo tauri signer sign "$TAR_FILE"; then
        echo "✅ Signature created with base64 encoded key"
    else
        echo "❌ Both methods failed. Key might be encrypted or in wrong format."
        echo ""
        echo "Debug info:"
        echo "Key first line: $(head -n 1 "$KEY_PATH")"
        echo ""
        echo "Try using the minisign method instead (sign-updater.sh)"
    fi
fi

cd -