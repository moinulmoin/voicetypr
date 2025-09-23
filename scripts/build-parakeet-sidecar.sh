#!/usr/bin/env bash
set -euo pipefail

# Build the Parakeet MLX sidecar as a standalone binary using uv + PyInstaller.

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SIDECAR_DIR="$ROOT_DIR/sidecar/parakeet"
DIST_DIR="$SIDECAR_DIR/dist"

cd "$SIDECAR_DIR"

# Ensure dependencies for the build group are installed
uv sync --group build

# Clean previous artifacts
rm -rf "$DIST_DIR"

# Detect OS and use appropriate spec file
OS_NAME=$(uname -s)
if [[ "$OS_NAME" == "Darwin" ]]; then
  echo "Building for macOS with background process configuration..."
  # Use macOS-specific spec file to prevent focus stealing
  uv run --group build pyinstaller \
    --clean \
    parakeet-sidecar-macos.spec
else
  echo "Building for $OS_NAME..."
  # Use default PyInstaller configuration for other platforms
  uv run --group build pyinstaller \
    --clean \
    --onefile \
    --name parakeet-sidecar \
    --hidden-import mlx._reprlib_fix \
    --collect-submodules mlx \
    --collect-submodules parakeet_mlx \
    --collect-data parakeet_mlx \
    --collect-data mlx \
    src/parakeet_sidecar/main.py
fi

# Create a target-suffixed copy for Tauri bundling
HOST_TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
BIN_PATH="$DIST_DIR/parakeet-sidecar"
SUFFIXED_PATH="$DIST_DIR/parakeet-sidecar-$HOST_TRIPLE"

if [[ -f "$BIN_PATH" ]];
then
  cp "$BIN_PATH" "$SUFFIXED_PATH"
  echo "Created suffixed binary: $SUFFIXED_PATH"
else
  echo "ERROR: sidecar binary not found at $BIN_PATH" >&2
  exit 1
fi

echo "Parakeet sidecar built at $DIST_DIR"
