# CI/CD Setup Guide

This guide explains how to use the CI/CD pipeline for VoiceTypr with automated releases and updates.

## Overview

The CI/CD pipeline consists of:
- **release-it**: Automated version management and changelog generation
- **GitHub Actions**: Cross-platform builds and releases
- **Tauri Updater**: Built-in auto-update functionality

## Prerequisites

1. **Generate Updater Keys**
   ```bash
   ./scripts/generate-updater-keys.sh
   ```
   
   This will:
   - Generate a public/private key pair
   - Save the private key to `~/.tauri/voicetypr.key`
   - Display the public key to add to `tauri.conf.json`

2. **Update Configuration**
   - Replace `YOUR_PUBLIC_KEY_HERE` in `src-tauri/tauri.conf.json` with your public key

3. **Configure GitHub Secrets**
   Add these secrets to your GitHub repository:
   - `TAURI_PRIVATE_KEY`: Contents of `~/.tauri/voicetypr.key`
   - `TAURI_KEY_PASSWORD`: The password you used when generating the keys

## Usage

### Local Development

#### Test Release Process
```bash
pnpm release:dry
```
This performs a dry run without making any changes.

#### Create a Release
```bash
# Automatic version bump based on commits
pnpm release

# Specific version bump
pnpm release:patch  # 0.1.0 → 0.1.1
pnpm release:minor  # 0.1.0 → 0.2.0
pnpm release:major  # 0.1.0 → 1.0.0
```

### Production Releases

1. Go to GitHub Actions → "Release" workflow
2. Click "Run workflow"
3. Select release type: `patch`, `minor`, or `major`
4. Click "Run workflow"

The workflow will:
1. Bump the version using release-it
2. Generate a changelog from commit messages
3. Create a git tag and push it
4. Build binaries for all platforms:
   - macOS (Intel + Apple Silicon)
   - Windows
   - Linux
5. Create a GitHub release with the binaries
6. Generate `latest.json` for the auto-updater

### Commit Message Convention

Follow the Angular convention for automatic changelog generation:

- `feat:` New features (triggers minor version bump)
- `fix:` Bug fixes (triggers patch version bump)
- `chore:` Maintenance tasks (no version bump)
- `docs:` Documentation changes (no version bump)
- `BREAKING CHANGE:` in commit body (triggers major version bump)

Examples:
```bash
git commit -m "feat: add voice activity detection"
git commit -m "fix: resolve audio recording issue on Windows"
git commit -m "chore: update dependencies"
```

## Auto-Updates

### How It Works

1. App checks for updates:
   - Automatically on startup (if configured)
   - Manually via Settings → About → Check for Updates

2. Update process:
   - Downloads `latest.json` from GitHub releases
   - Verifies update signature
   - Shows native update dialog
   - Downloads and installs in background
   - Prompts user to restart

### Testing Updates

1. Build and install a release version
2. Create a new release with a higher version
3. Open the app and check for updates
4. Verify the update dialog appears and works correctly

## Troubleshooting

### Release Workflow Fails

1. **Check GitHub Secrets**: Ensure `TAURI_PRIVATE_KEY` and `TAURI_KEY_PASSWORD` are set
2. **Verify Permissions**: Repository needs write permissions for Actions
3. **Check Build Logs**: Look for platform-specific build errors

### Updates Not Working

1. **Verify Public Key**: Ensure the public key in `tauri.conf.json` matches your generated key
2. **Check Endpoint**: Verify the update endpoint URL is correct
3. **Test Manually**: Download `latest.json` and check its format

### Key Generation Issues

If you lose your keys or need to regenerate:
1. Generate new keys with the script
2. Update the public key in `tauri.conf.json`
3. Update GitHub Secrets with new private key
4. All future releases will use the new keys

## Security Notes

- **Never commit private keys** to the repository
- Keep your key password secure
- Rotate keys periodically for security
- Use GitHub's secret scanning to prevent accidental exposure

## Release Checklist

Before releasing:
- [ ] All tests pass (`pnpm test:all`)
- [ ] Code is linted (`pnpm lint`)
- [ ] TypeScript has no errors (`pnpm typecheck`)
- [ ] Changelog is up to date
- [ ] Version number makes sense
- [ ] GitHub Secrets are configured
- [ ] Previous release tested successfully