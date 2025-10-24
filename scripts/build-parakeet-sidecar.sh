#!/usr/bin/env bash
set -euo pipefail

# Smart sidecar build wrapper
# - Prefer Swift/FluidAudio sidecar if present (macOS arm64)
# - Fallback to legacy Python MLX build if Swift sidecar not found

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SWIFT_DIR="$ROOT_DIR/sidecar/parakeet-swift"
PY_DIR="$ROOT_DIR/sidecar/parakeet"

if [[ -d "$SWIFT_DIR" ]]; then
  # Build Swift sidecar (macOS only)
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "[sidecar] Swift sidecar present but non-macOS host; skipping build"
    exit 0
  fi

  echo "[sidecar] Building Swift Parakeet sidecar (arm64 release)..."
  pushd "$SWIFT_DIR" > /dev/null
  swift build -c release --arch arm64
  BIN_DIR=$(swift build -c release --arch arm64 --show-bin-path 2>/dev/null || echo ".build/arm64-apple-macosx/release")
  SRC_BIN_NAME="ParakeetSidecar"
  SRC_BIN_PATH="$BIN_DIR/$SRC_BIN_NAME"
  mkdir -p dist
  cp "$SRC_BIN_PATH" "dist/parakeet-sidecar-aarch64-apple-darwin"
  chmod +x "dist/parakeet-sidecar-aarch64-apple-darwin"
  ln -sfn "parakeet-sidecar-aarch64-apple-darwin" "dist/parakeet-sidecar"
  echo "[sidecar] Swift sidecar ready at $SWIFT_DIR/dist"
  popd > /dev/null
  exit 0
fi

if [[ -d "$PY_DIR" ]]; then
  echo "[sidecar] Building legacy Python MLX Parakeet sidecar..."
  SIDECAR_DIR="$PY_DIR"
  DIST_DIR="$SIDECAR_DIR/dist"
  cd "$SIDECAR_DIR"

  if ! command -v uv >/dev/null 2>&1; then
    echo "[sidecar] 'uv' not installed; skipping Python sidecar build"
    exit 0
  fi

  uv sync --group build
  rm -rf "$DIST_DIR"

  OS_NAME=$(uname -s)
  if [[ "$OS_NAME" == "Darwin" ]]; then
    uv run --group build pyinstaller --clean parakeet-sidecar-macos.spec
  else
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

  HOST_TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
  BIN_PATH="$DIST_DIR/parakeet-sidecar"
  SUFFIXED_PATH="$DIST_DIR/parakeet-sidecar-$HOST_TRIPLE"

  if [[ -f "$BIN_PATH" ]]; then
    cp "$BIN_PATH" "$SUFFIXED_PATH"
    echo "[sidecar] Created suffixed binary: $SUFFIXED_PATH"
  else
    echo "[sidecar] ERROR: sidecar binary not found at $BIN_PATH" >&2
    exit 1
  fi

  echo "[sidecar] Python sidecar built at $DIST_DIR"
  exit 0
fi

echo "[sidecar] No sidecar sources found; nothing to build"
exit 0
