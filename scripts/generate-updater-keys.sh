#!/bin/bash

echo "========================================="
echo "Tauri Updater Key Generation"
echo "========================================="
echo ""
echo "This script will generate signing keys for the Tauri updater."
echo "You'll need to:"
echo "1. Enter a password to protect the private key"
echo "2. Save the public key to tauri.conf.json"
echo "3. Add the private key and password to GitHub Secrets"
echo ""
echo "Press Enter to continue..."
read

# Create directory if it doesn't exist
mkdir -p ~/.tauri

# Generate the keys
echo "Generating keys..."
pnpm tauri signer generate -w ~/.tauri/voicetypr.key

echo ""
echo "========================================="
echo "IMPORTANT: Next Steps"
echo "========================================="
echo ""
echo "1. Copy the public key shown above"
echo "2. Replace 'YOUR_PUBLIC_KEY_HERE' in src-tauri/tauri.conf.json with the public key"
echo "3. Add these GitHub Secrets:"
echo "   - TAURI_PRIVATE_KEY: Contents of ~/.tauri/voicetypr.key"
echo "   - TAURI_KEY_PASSWORD: The password you just entered"
echo ""
echo "To view your private key:"
echo "cat ~/.tauri/voicetypr.key"
echo ""
echo "SECURITY WARNING: Never commit the private key to your repository!"