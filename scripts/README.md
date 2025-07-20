# VoiceTypr Release Scripts

## Scripts Overview

- `release.sh` - Main release script that handles version bumping, building, and GitHub release creation
- `fix-release-archives.sh` - Fixes macOS tar.gz archives by removing AppleDouble files
- `create-latest-json.js` - Creates the combined latest.json for the updater
- Other scripts - Various build configurations for different scenarios

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