#!/bin/bash
set -e

# Configuration
RELEASE_DIR="release-1.0.0"
TAR_FILE="VoiceTypr_1.0.0_universal.app.tar.gz"
PUBLIC_KEY="dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDFBRDM5NUI0NkY3Q0MzMUYKUldRZnczeHZ0SlhUR2hwL09YK2lTSHg5T2FrZmxmMlV5QitVR2VxaW9UcERQaWZUYnl1WjErdWEK"

# This is from your tauri.conf.json
echo "Public key from tauri.conf.json:"
echo "$PUBLIC_KEY"
echo ""

# Check if signature exists
if [ ! -f "$RELEASE_DIR/${TAR_FILE}.sig" ]; then
    echo "❌ Signature file not found: $RELEASE_DIR/${TAR_FILE}.sig"
    exit 1
fi

# Display signature info
echo "Signature file content:"
cat "$RELEASE_DIR/${TAR_FILE}.sig"
echo ""

# Try to verify with minisign if available
if command -v minisign &> /dev/null; then
    echo "Verifying with minisign..."
    
    # Create a temporary public key file
    echo "$PUBLIC_KEY" | base64 -d > /tmp/voicetypr.pub
    
    cd "$RELEASE_DIR"
    if minisign -V -p /tmp/voicetypr.pub -m "$TAR_FILE"; then
        echo "✅ Signature verification successful!"
    else
        echo "❌ Signature verification failed"
    fi
    cd -
    
    rm -f /tmp/voicetypr.pub
else
    echo "minisign not installed, skipping verification"
    echo "Install with: brew install minisign"
fi

# Also check the latest.json format
echo ""
echo "Checking latest.json..."
if [ -f "$RELEASE_DIR/latest.json" ]; then
    # Read the signature from the .sig file
    SIG_CONTENT=$(cat "$RELEASE_DIR/${TAR_FILE}.sig" | tail -n 1)
    
    echo "Update latest.json with this signature:"
    echo "$SIG_CONTENT"
    echo ""
    echo "Current latest.json:"
    cat "$RELEASE_DIR/latest.json"
fi