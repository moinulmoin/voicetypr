# VoiceTypr Windows Release Script
# This script is designed to run AFTER the macOS release-separate.sh script
# It assumes version has already been bumped and tagged by the macOS script
#
# Usage: .\scripts\release-windows.ps1 [version]
# If version is not provided, it will be read from package.json

param(
    [string]$Version,
    [switch]$Help,
    [switch]$SkipBuild
)

# Colors for PowerShell output
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Color
}

function Write-Success { param([string]$Message) Write-ColorOutput "[OK] $Message" "Green" }
function Write-Warning { param([string]$Message) Write-ColorOutput "[WARN] $Message" "Yellow" }
function Write-Error { param([string]$Message) Write-ColorOutput "✗ $Message" "Red" }
function Write-Info { param([string]$Message) Write-ColorOutput "[INFO] $Message" "Cyan" }
function Write-Step { param([string]$Message) Write-ColorOutput "🚀 $Message" "Magenta" }

# Show help
if ($Help) {
    Write-Host @"
VoiceTypr Windows Release Script

This script builds Windows MSI installer and update artifacts for VoiceTypr.
It is designed to run AFTER the macOS release script has created the release.

Usage:
  .\scripts\release-windows.ps1                # Use version from package.json
  .\scripts\release-windows.ps1 1.4.0         # Use specific version
  .\scripts\release-windows.ps1 -SkipBuild    # Skip build, use existing artifacts
  .\scripts\release-windows.ps1 -Help         # Show this help

Requirements:
- Node.js and pnpm installed
- Rust toolchain with windows targets
- GitHub CLI (gh) installed and authenticated
- TAURI_SIGNING_PRIVATE_KEY environment variable (optional, for update signatures)

The script will:
1. Read version from package.json (or use provided version)
2. Verify the GitHub release exists
3. Build Windows NSIS installer
4. Create Tauri update artifacts (.nsis.zip and signatures)
6. Download and update latest.json from GitHub release
7. Upload all Windows artifacts to the existing release

Environment Variables:
- TAURI_SIGNING_PRIVATE_KEY: Private key for signing update artifacts
- TAURI_SIGNING_PRIVATE_KEY_PASSWORD: Password for the private key (if needed)
- GITHUB_TOKEN: GitHub token for API access (gh CLI should handle this)
"@
    exit 0
}

Write-Step "Starting VoiceTypr Windows Release Process"

# Remember initial directory so we can return even on errors
$InitialDir = Get-Location

# Error handling
$ErrorActionPreference = "Stop"

# Get version
if (-not $Version) {
    Write-Info "Reading version from package.json..."
    try {
        $packageJson = Get-Content "package.json" | ConvertFrom-Json
        $Version = $packageJson.version
        Write-Success "Version detected: $Version"
    } catch {
        Write-Error "Failed to read version from package.json: $($_.Exception.Message)"
        exit 1
    }
} else {
    Write-Info "Using provided version: $Version"
}

# Validate version format
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    Write-Error "Invalid version format: $Version (expected x.y.z)"
    exit 1
}

$ReleaseTag = "v$Version"
$OutputDir = "release-windows-$Version"

# Check if GitHub CLI is available
try {
    $null = Get-Command "gh" -ErrorAction Stop
    Write-Success "GitHub CLI found"
} catch {
    Write-Error "GitHub CLI (gh) not found. Please install it from https://cli.github.com/"
    exit 1
}

# Verify GitHub authentication
Write-Info "Checking GitHub authentication..."
try {
    $null = gh auth status 2>$null
    Write-Success "GitHub CLI authenticated"
} catch {
    Write-Error "GitHub CLI not authenticated. Run: gh auth login"
    exit 1
}

# Check if release exists
Write-Info "Checking if release $ReleaseTag exists..."
try {
    $releaseInfo = gh release view $ReleaseTag --json id,name 2>$null | ConvertFrom-Json
    Write-Success "Found existing release: $($releaseInfo.name)"
} catch {
    Write-Error "Release $ReleaseTag not found. Please run the macOS release script first."
    exit 1
}

# Check for Rust and required targets
Write-Info "Checking Rust toolchain..."
try {
    $null = Get-Command "rustup" -ErrorAction Stop
    Write-Success "Rust toolchain found"
    
    # Install Windows target if not already installed
    Write-Info "Ensuring Windows target is installed..."
    rustup target add x86_64-pc-windows-msvc
    Write-Success "Windows target ready"
} catch {
    Write-Error "Rust toolchain not found. Please install from https://rustup.rs/"
    exit 1
}

# Check for required tools
Write-Info "Checking required tools..."
try {
    $null = Get-Command "pnpm" -ErrorAction Stop
    Write-Success "pnpm found"
} catch {
    Write-Error "pnpm not found. Please install it: npm install -g pnpm"
    exit 1
}

# Create output directory
if (Test-Path $OutputDir) {
    Remove-Item $OutputDir -Recurse -Force
}
New-Item -ItemType Directory -Path $OutputDir | Out-Null
Write-Success "Created output directory: $OutputDir"

# Check for signing configuration
$HasSigningKey = $false
$keyPath = "$env:USERPROFILE\.tauri\voicetypr.key"

# If needed you can export TAURI_SIGNING_PRIVATE_KEY_PATH before running this script
# $env:TAURI_SIGNING_PRIVATE_KEY_PATH = "C:\Users\moinu\.tauri\voicetypr.key"

# Auto-detect and set key path if file exists
if (Test-Path $keyPath) {
    $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $keyPath
    $HasSigningKey = $true
    Write-Success "Tauri signing key found at: $keyPath"
    Write-Info "Set TAURI_SIGNING_PRIVATE_KEY_PATH environment variable"
} elseif ($env:TAURI_SIGNING_PRIVATE_KEY -or $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    $HasSigningKey = $true
    Write-Success "Tauri signing key configured via environment variable"
} else {
    Write-Warning "No Tauri signing key found - update signatures will not be generated"
    Write-Warning "To enable signing:"
    Write-Warning "1. Generate keys: cargo tauri signer generate -w `"$env:USERPROFILE\.tauri\voicetypr.key`""
    Write-Warning "2. The script will auto-detect the key at the standard location"
}

# Build Windows NSIS installer
if (-not $SkipBuild) {
    Write-Step "Building Windows NSIS installer..."
    try {
        # Run Tauri build with verbose output
        Write-Info "Building Tauri application (this may take several minutes)..."
        
        # Set environment to skip signing if no key
        if (-not $HasSigningKey) {
            $env:TAURI_SIGNING_PRIVATE_KEY = ""
            $env:TAURI_SIGNING_PRIVATE_KEY_PATH = ""
        }
        
        # Run build with full output
        & pnpm tauri build --verbose
        
        if ($LASTEXITCODE -ne 0) {
            Write-Error "Tauri build failed with exit code: $LASTEXITCODE"
            exit 1
        }
        Write-Success "NSIS build completed"
    } catch {
        Write-Error "Failed to build: $($_.Exception.Message)"
        exit 1
    }
} else {
    Write-Info "Skipping build step (using existing artifacts)"
}

# Find the built NSIS installer
$NsisPath = Get-ChildItem -Path "src-tauri\target\release\bundle\nsis" -Filter "VoiceTypr_${Version}_x64-setup.exe" | Select-Object -First 1
if (-not $NsisPath) {
    Write-Error "NSIS installer not found in build output"
    exit 1
}

Write-Success "Found NSIS installer: $($NsisPath.Name)"

# Copy NSIS installer (already has good naming: VoiceTypr_1.4.0_x64-setup.exe)
$NewNsisPath = Join-Path $OutputDir $NsisPath.Name
Copy-Item $NsisPath.FullName $NewNsisPath
Write-Success "Copied NSIS installer: $($NsisPath.Name)"

# Create update artifacts
Write-Step "Creating Tauri update artifacts..."

# Create .nsis.zip for updater
$NsisZipPath = "$NewNsisPath.zip"
try {
    Compress-Archive -Path $NewNsisPath -DestinationPath $NsisZipPath -Force
    Write-Success "Created update archive: $(Split-Path $NsisZipPath -Leaf)"
} catch {
    Write-Error "Failed to create NSIS zip: $($_.Exception.Message)"
    exit 1
}

# Sign update artifacts if signing key is available
$NsisZipSignature = "SIGNATURE_PLACEHOLDER"
if ($HasSigningKey) {
    Write-Info "Signing update artifacts..."
    try {
        # Get absolute path for the zip file
        $zipFullPath = (Get-Item $NsisZipPath).FullName
        
        # Try multiple approaches to make signing work on Windows
        $keyContent = Get-Content $env:TAURI_SIGNING_PRIVATE_KEY_PATH -Raw
        
        # Approach 1: Use -f flag with key file path (most reliable)
        Write-Info "Attempting to sign with: pnpm tauri signer sign -f $keyPath -p '' <file>"
        $signOutput = & pnpm tauri signer sign -f $keyPath $zipFullPath 2>&1
        
        if ($LASTEXITCODE -ne 0) {
            # Approach 2: Set environment variables (npm version uses TAURI_PRIVATE_KEY)
            Write-Warning "Direct signing failed, trying with environment variables..."
            $env:TAURI_PRIVATE_KEY = $keyContent
            $env:TAURI_PRIVATE_KEY_PATH = $env:TAURI_SIGNING_PRIVATE_KEY_PATH
            $env:TAURI_SIGNING_PRIVATE_KEY = $keyContent
            
            $signOutput = & pnpm tauri signer sign $zipFullPath 2>&1
        }
        
        if ($LASTEXITCODE -ne 0) {
            Write-Error "Failed to sign. Output: $signOutput"
            exit 1
        }
        
        if ($LASTEXITCODE -eq 0 -and (Test-Path "$NsisZipPath.sig")) {
            $NsisZipSignature = Get-Content "$NsisZipPath.sig" -Raw
            $NsisZipSignature = $NsisZipSignature.Trim()
            Write-Success "Update artifact signed successfully"
        } else {
            Write-Error "Failed to sign update artifact. Output: $signOutput"
            exit 1
        }
    } catch {
        Set-Location $InitialDir -ErrorAction SilentlyContinue
        Write-Error "Error during signing: $($_.Exception.Message)"
        exit 1
    }
} else {
    Write-Warning "Skipping signature generation (no signing key configured)"
}

# Download and update latest.json
Write-Step "Updating latest.json with Windows platform..."
try {
    # Download existing latest.json
    $latestJsonPath = Join-Path $OutputDir "latest.json"
    gh release download $ReleaseTag --pattern "latest.json" --dir $OutputDir
    
    if (-not (Test-Path $latestJsonPath)) {
        Write-Error "Failed to download latest.json from release"
        exit 1
    }
    
    # Parse existing JSON
    $latestJson = Get-Content $latestJsonPath | ConvertFrom-Json
    Write-Success "Downloaded and parsed existing latest.json"
    
    # Add Windows platform
    $windowsPlatform = @{
        signature = $NsisZipSignature
        url = "https://github.com/moinulmoin/voicetypr/releases/download/$ReleaseTag/$(Split-Path $NsisZipPath -Leaf)"
    }
    
    # Ensure platforms object exists
    if (-not $latestJson.platforms) {
        $latestJson | Add-Member -NotePropertyName "platforms" -NotePropertyValue @{}
    }
    
    # Add windows-x86_64 platform
    $latestJson.platforms | Add-Member -NotePropertyName "windows-x86_64" -NotePropertyValue $windowsPlatform -Force
    
    # Save updated JSON
    $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
    Write-Success "Added Windows platform to latest.json"
    
} catch {
    Write-Error "Failed to update latest.json: $($_.Exception.Message)"
    exit 1
}

# Upload all Windows artifacts to release
Write-Step "Uploading Windows artifacts to GitHub release..."
try {
    # Upload NSIS installer
    Write-Info "Uploading NSIS installer..."
    gh release upload $ReleaseTag $NewNsisPath --clobber
    Write-Success "Uploaded: $(Split-Path $NewNsisPath -Leaf)"
    
    # Upload NSIS.zip
    Write-Info "Uploading NSIS update archive..."
    gh release upload $ReleaseTag $NsisZipPath --clobber
    Write-Success "Uploaded: $(Split-Path $NsisZipPath -Leaf)"
    
    # Upload signature if it exists
    if (Test-Path "$NsisZipPath.sig") {
        Write-Info "Uploading NSIS signature..."
        gh release upload $ReleaseTag "$NsisZipPath.sig" --clobber
        Write-Success "Uploaded: $(Split-Path "$NsisZipPath.sig" -Leaf)"
    }
    
    # Upload updated latest.json
    Write-Info "Uploading updated latest.json..."
    gh release upload $ReleaseTag $latestJsonPath --clobber
    Write-Success "Uploaded: latest.json"
    
} catch {
    Write-Error "Failed to upload artifacts: $($_.Exception.Message)"
    exit 1
}

# Summary
Write-Step "Windows Release Complete!"
Write-Success "Successfully created and uploaded Windows release artifacts for v$Version"
Write-Host ""
Write-Info "Windows Release Artifacts:"
Get-ChildItem $OutputDir | ForEach-Object {
    $size = if ($_.Length -gt 1MB) { "{0:N2} MB" -f ($_.Length / 1MB) } else { "{0:N2} KB" -f ($_.Length / 1KB) }
    Write-Host "   $($_.Name) ($size)" -ForegroundColor White
}

Write-Host ""
Write-Info "Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/$ReleaseTag"

if ($HasSigningKey) {
    Write-Success "Update signatures generated - auto-updater ready"
} else {
    Write-Warning "No update signatures - auto-updater won`'t work"
}

Write-Host ""
Write-Info "Next Steps:"
Write-Host "1. Test the MSI installer on a clean Windows machine" -ForegroundColor Yellow
Write-Host "2. Verify the Tauri updater works with the new artifacts" -ForegroundColor Yellow
Write-Host "3. Update release notes if needed" -ForegroundColor Yellow
Write-Host "4. Publish the release when ready" -ForegroundColor Yellow

Write-Success "Windows release process completed successfully!"