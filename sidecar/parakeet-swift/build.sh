#!/bin/bash
set -e

# Swift Parakeet Sidecar Build Script
# Builds the Swift sidecar binary for VoiceTypr following Tauri v2 sidecar conventions

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$SCRIPT_DIR/dist"

echo "🔨 Building Swift Parakeet Sidecar..."

# Determine build configuration
BUILD_CONFIG="${1:-release}"
echo "📦 Build configuration: $BUILD_CONFIG"

# Clean previous builds (but keep dist directory for incremental builds)
echo "🧹 Cleaning previous builds..."
rm -rf "$SCRIPT_DIR/.build"

# Build Swift package
echo "🏗️  Compiling Swift package..."
cd "$SCRIPT_DIR"
swift build -c "$BUILD_CONFIG"

# Create dist directory
mkdir -p "$DIST_DIR"

# Determine Rust target triple (Tauri expects this format)
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    ARCH="aarch64"
fi

# Map to Rust target triple
case "$(uname -s)" in
    Darwin)
        TARGET_TRIPLE="${ARCH}-apple-darwin"
        ;;
    Linux)
        TARGET_TRIPLE="${ARCH}-unknown-linux-gnu"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        TARGET_TRIPLE="${ARCH}-pc-windows-msvc"
        ;;
    *)
        echo "❌ Unsupported platform: $(uname -s)"
        exit 1
        ;;
esac

echo "🖥️  Target triple: $TARGET_TRIPLE"

# Copy binary with correct name for Tauri
if [ "$BUILD_CONFIG" = "release" ]; then
    BUILD_PATH=".build/release/ParakeetSidecar"
else
    BUILD_PATH=".build/debug/ParakeetSidecar"
fi

if [ ! -f "$BUILD_PATH" ]; then
    echo "❌ Error: Binary not found at $BUILD_PATH"
    exit 1
fi

# Copy with Tauri-expected naming: parakeet-sidecar-$TARGET_TRIPLE
OUTPUT_PATH="$DIST_DIR/parakeet-sidecar-$TARGET_TRIPLE"
cp "$BUILD_PATH" "$OUTPUT_PATH"
echo "✅ Binary copied to: $OUTPUT_PATH"

# Make executable
chmod +x "$OUTPUT_PATH"

# Verify binary
echo "🔍 Verifying binary..."
if echo '{"type":"status"}' | "$OUTPUT_PATH" 2>/dev/null | grep -q '"type"'; then
    echo "✅ Binary verification successful!"
else
    echo "⚠️  Warning: Binary verification failed (this is OK if not on macOS 13+)"
fi

# Print size
SIZE=$(du -h "$OUTPUT_PATH" | cut -f1)
echo "📊 Binary size: $SIZE"

# List all binaries in dist
echo ""
echo "📁 Binaries in dist directory:"
ls -lh "$DIST_DIR"

echo ""
echo "🎉 Swift sidecar build complete!"
echo "   Binary: $OUTPUT_PATH"
echo "   Tauri will automatically resolve this when spawning 'parakeet-sidecar'"
