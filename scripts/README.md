# VoiceTypr Release Scripts

## Scripts Overview

### Main Release Scripts
- `release-separate.sh` - macOS release script (creates version, builds both architectures, creates GitHub release)
- `release-windows.ps1` - Windows release script (single CPU-safe installer with bundled optional Vulkan GPU sidecar)

### Supporting Scripts
- `fix-release-archives.sh` - Fixes macOS tar.gz archives by removing AppleDouble files
- `create-latest-json.js` - Creates the combined latest.json for the updater
- Other scripts - Various build configurations for different scenarios

## Cross-Platform Release Workflow

The recommended release process is:

1. **macOS Release** (creates the initial release):
   ```bash
   ./scripts/release-separate.sh [patch|minor|major]
   ```
   - Bumps version in package.json
   - Updates Cargo.toml and tauri.conf.json
   - Creates git tag
   - Builds both Intel (x64) and Apple Silicon (aarch64) binaries
   - Creates GitHub draft release with macOS artifacts
   - Generates initial latest.json with macOS platforms

2. **Windows Release** (adds to existing release):
   ```powershell
   .\scripts\release-windows.ps1
   ```
   - Reads version from package.json (or uses provided version)
   - Verifies the GitHub release exists
   - Builds one Windows NSIS installer
   - Keeps `voicetypr.exe` CPU-safe and free of `vulkan-1.dll`
   - Bundles the optional Vulkan Whisper sidecar and Vulkan Runtime installer resource
   - Signs the installer, updates latest.json, and uploads Windows artifacts

### Environment Variables

**macOS (release-separate.sh)**:
- `APPLE_SIGNING_IDENTITY` - Apple Developer signing identity
- `APPLE_API_KEY` + `APPLE_API_ISSUER` - API key authentication (preferred)
- OR `APPLE_ID` + `APPLE_PASSWORD` + `APPLE_TEAM_ID` - Apple ID authentication
- `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH` - Tauri update signing

**Windows (release-windows.ps1)**:
- `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH` - Tauri update signing
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` - Password for signing key (if needed)
- `GITHUB_TOKEN` - GitHub authentication (usually handled by gh CLI)
- `VULKAN_SDK` - Required when building Windows releases; used only for the optional GPU sidecar/runtime resource
- `VULKAN_RUNTIME_VERSION` - Optional pinned Vulkan Runtime installer version; defaults to `VULKAN_VERSION`/SDK folder name

## Important: AppleDouble Files Fix

### The Problem
macOS creates hidden AppleDouble files (prefixed with `._`) when creating tar archives. These files store extended attributes and resource forks. When Tauri's updater tries to unpack these files, it fails with errors like:

```
failed to unpack `._voicetypr.app` into `/var/folders/.../T/tauri_updated_app.../`
```

### The Solution
1. **Environment Variable**: Set `COPYFILE_DISABLE=1` in `.cargo/config.toml` to prevent creation during build
2. **Post-Build Fix**: The `fix-release-archives.sh` script repacks archives without AppleDouble files
3. **Release Process**: The main `release.sh` automatically calls the fix script after building

### Manual Fix (if needed)
If you need to fix an existing archive:
```bash
COPYFILE_DISABLE=1 tar -czf fixed.tar.gz --exclude='._*' --exclude='.DS_Store' VoiceTypr.app
```

This ensures the Tauri updater can successfully unpack and install updates on all macOS systems.