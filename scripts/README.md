# VoiceTypr Release Scripts

## Scripts Overview

### Main Release Scripts
- `release-separate.sh` - macOS release script (creates version, builds both architectures, creates GitHub release)
- `release-windows.ps1` - Windows release script (builds NSIS installer, updates existing release)
- `release-windows.bat` - Batch wrapper for the PowerShell script

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

2. **Windows x86_64 Release** (adds to existing release):
   ```powershell
   .\scripts\release-windows.ps1 [version]
   ```
   OR
   ```batch
   scripts\release-windows.bat [version]
   ```
   - Reads version from package.json (or uses provided version)
   - Verifies the GitHub release exists
   - Builds the Windows x64 NSIS installer (CPU-safe main app + optional x86_64 Vulkan sidecar)
   - Uses `src-tauri/tauri.windows.conf.json`, which is x86_64-only (Windows ARM64 stays CPU-only)
   - Bundles VC++ and Vulkan Runtime installers as best-effort post-install steps
   - Signs the installer and updates latest.json with the Windows platform
   - Uploads the installer, signature, and latest.json to the existing release

### Environment Variables

**macOS (release-separate.sh)**:
- `APPLE_SIGNING_IDENTITY` - Apple Developer signing identity
- `APPLE_API_KEY` + `APPLE_API_ISSUER` - API key authentication (preferred)
- OR `APPLE_ID` + `APPLE_PASSWORD` + `APPLE_TEAM_ID` - Apple ID authentication
- `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH` - Tauri update signing

**Windows x86_64 (release-windows.ps1)**:
- `VULKAN_SDK` - Path to Vulkan SDK (required to build the optional x64 GPU sidecar)
- `VULKAN_RUNTIME_VERSION` or `VULKAN_VERSION` - Vulkan Runtime version for bundling (defaults to SDK folder name)
- `CARGO_TARGET_DIR` - Optional short build output path (honored for sidecar and main app builds)
- `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH` - Tauri update signing
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` - Password for signing key (if needed)
- `GITHUB_TOKEN` - GitHub authentication (usually handled by gh CLI)

### Windows x86_64 Build Prerequisites

This release path builds an x64 installer with an optional x86_64 Vulkan sidecar.
Windows ARM64 builds are CPU-only and must not use `src-tauri/tauri.windows.conf.json`.

**1. Vulkan SDK**
Download from https://vulkan.lunarg.com/sdk/home and ensure `VULKAN_SDK` is set.

**2. FFmpeg Sidecar Binaries**
Place the following files in `sidecar/ffmpeg/dist/`:
- `ffmpeg.exe` and `ffprobe.exe` (base binaries)
- `ffmpeg-x86_64-pc-windows-msvc.exe` and `ffprobe-x86_64-pc-windows-msvc.exe`
- `ffmpeg.exe-x86_64-pc-windows-msvc.exe` and `ffprobe.exe-x86_64-pc-windows-msvc.exe`

These are not tracked in git due to their size (~100MB each).

**3. Windows MAX_PATH Limitation**
Windows has a 260-character path limit. When using git worktrees or long paths, set a short target directory for both the sidecar and main app builds:
```powershell
$env:CARGO_TARGET_DIR = "C:\tmp\vt-target"
.\scripts\release-windows.ps1 -SkipPublish
```
This is especially important for worktrees where paths become very long.

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