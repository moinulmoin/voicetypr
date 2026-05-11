#!/bin/bash
# Post-build script to ensure the macOS app is code-signed
# This is required for proper firewall behavior on macOS

set -e

APP_PATH="src-tauri/target/release/bundle/macos/Voicetypr.app"

if [[ ! -d "$APP_PATH" ]]; then
    echo "App bundle not found at $APP_PATH"
    echo "Run 'pnpm tauri build' first"
    exit 1
fi

# Check current signing status
echo "Checking current signing status..."
if codesign -dv "$APP_PATH" 2>&1 | grep -q "code object is not signed"; then
    echo "App is not signed. Signing with ad-hoc signature..."
    codesign --force --deep --sign - "$APP_PATH"
    echo "App signed successfully!"
else
    echo "App is already signed:"
    codesign -dv "$APP_PATH" 2>&1 | head -5
fi

# Verify signature
echo ""
echo "Verifying signature..."
codesign --verify --deep --strict "$APP_PATH" && echo "Signature verified OK"
