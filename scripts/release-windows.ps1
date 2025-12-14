# Windows Release Script - Single smart build with GPU support

param(
    [string]$Version,
    [switch]$Help,
    [switch]$SkipBuild,
    [switch]$SkipPublish
)

# Colors
function Write-Success($Message) { Write-Host "[OK] $Message" -ForegroundColor Green }
function Write-Error($Message) { Write-Host "[ERROR] $Message" -ForegroundColor Red }
function Write-Info($Message) { Write-Host "[INFO] $Message" -ForegroundColor Cyan }
function Write-Step($Message) { Write-Host "`n==> $Message" -ForegroundColor Magenta }

function Require-Command($Command) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        Write-Error "$Command not found in PATH"
        exit 1
    }
}

function Require-File($Path) {
    if (-not (Test-Path $Path)) {
        Write-Error "Required file not found: $Path"
        exit 1
    }
}

if ($Help) {
    Write-Host @"
Windows Release Script

Builds a single smart installer that:
- Detects GPU capability
- Informs users about GPU acceleration
- Falls back to CPU if needed
- Always works!

Usage:
  .\scripts\release-windows.ps1                    # Build and upload
  .\scripts\release-windows.ps1 -SkipBuild         # Upload existing build
  .\scripts\release-windows.ps1 -SkipPublish       # Build only, don't upload
  .\scripts\release-windows.ps1 -Help              # Show this help
"@
    exit 0
}

# Get version
if (-not $Version) {
    $packageJson = Get-Content "package.json" | ConvertFrom-Json
    $Version = $packageJson.version
}

Write-Step "VoiceTypr Windows Release v$Version"

$ReleaseTag = "v$Version"
$OutputDir = "release-windows-$Version"

# Create output directory
if (-not (Test-Path $OutputDir)) {
    Write-Info "Creating output directory: $OutputDir"
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
    if (-not (Test-Path $OutputDir)) {
        Write-Error "Failed to create output directory: $OutputDir"
        exit 1
    }
    Write-Success "Output directory created"
} else {
    Write-Info "Output directory already exists: $OutputDir"
}

Require-Command cargo
Require-Command pnpm
Require-File "package.json"

if (-not $SkipPublish) {
    Require-Command gh
}

# Build single smart installer
if (-not $SkipBuild) {
    Write-Step "Building VoiceTypr with GPU support..."
    
    # Check for Vulkan SDK
    if (-not $env:VULKAN_SDK) {
        Write-Error "VULKAN_SDK not set! Build requires Vulkan SDK."
        Write-Info "Download from: https://vulkan.lunarg.com/sdk/home"
        exit 1
    }
    
    # Clean to ensure fresh build
    cargo clean --manifest-path src-tauri\Cargo.toml
    
    # Build with Vulkan enabled by default
    Write-Info "Building with Vulkan support enabled..."
    # Version comes from Cargo.toml, no override needed
    pnpm tauri build --ci
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed!"
        exit 1
    }
    
    # Copy installer
    $installer = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        Write-Error "No installer found in src-tauri\target\release\bundle\nsis\"
        exit 1
    }
    Write-Info "Found installer: $($installer.FullName)"
    
    # Ensure output directory exists before copying
    if (-not (Test-Path $OutputDir)) {
        Write-Info "Re-creating output directory: $OutputDir"
        New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
    }
    
    $installerPath = "$OutputDir\VoiceTypr_${Version}_x64-setup.exe"
    Write-Info "Copying to: $installerPath"
    Copy-Item $installer.FullName $installerPath -Force
    
    if (Test-Path $installerPath) {
        Write-Success "Smart installer built successfully!"
    } else {
        Write-Error "Failed to copy installer to output directory"
        exit 1
    }
    
    # Sign installer directly if key available
    $keyPath = "$env:USERPROFILE\.tauri\voicetypr.key"
    $signature = ""
    if (Test-Path $keyPath) {
        Write-Info "Signing installer for updates..."
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $keyPath
        
        if ($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
            & pnpm tauri signer sign -f $keyPath -p $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD $installerPath
        } else {
            & pnpm tauri signer sign -f $keyPath -p "" $installerPath
        }
        
        if (Test-Path "$installerPath.sig") {
            Write-Success "Installer signed"
            # Read signature for latest.json - ensure proper formatting
            $signature = (Get-Content "$installerPath.sig" -Raw).Trim()
            # Remove any potential line breaks within the signature
            $signature = $signature -replace "`r`n", "" -replace "`n", ""
            Write-Info "Signature captured: $($signature.Substring(0, [Math]::Min(50, $signature.Length)))..."
        } else {
            Write-Error "Failed to sign installer (missing .sig file)"
            exit 1
        }
    } else {
        Write-Error "No signing key found at $keyPath (required for auto-updates)"
        exit 1
    }
    
    # Update latest.json with Windows platform
    Write-Info "Updating latest.json with Windows platform..."
    
    $latestJsonPath = "$OutputDir\latest.json"
    
    # Try to download existing latest.json from GitHub release
    Write-Info "Checking for existing latest.json in release..."
    try {
        gh release download $ReleaseTag -p "latest.json" -D $OutputDir --clobber 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path $latestJsonPath)) { Write-Success "Downloaded existing latest.json" }
    } catch {
        Write-Info "Error checking for latest.json: $_"
    }
    
    if (Test-Path $latestJsonPath) {
        # Read existing latest.json
        $latestJson = Get-Content $latestJsonPath -Raw | ConvertFrom-Json
        
        # Add Windows platform
        if (-not $latestJson.platforms) {
            $latestJson | Add-Member -NotePropertyName "platforms" -NotePropertyValue @{} -Force
        }
        
        # Use Add-Member to safely add the windows platform
        $windowsPlatform = @{
            signature = $signature
            url = "https://github.com/moinulmoin/voicetypr/releases/download/$ReleaseTag/VoiceTypr_${Version}_x64-setup.exe"
        }
        $latestJson.platforms | Add-Member -NotePropertyName "windows-x86_64" -NotePropertyValue $windowsPlatform -Force
        
        # Save updated latest.json
        $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
        Write-Success "Updated latest.json with Windows platform"
    } else {
        # Create new latest.json if it doesn't exist (Windows-only release)
        Write-Info "Creating new latest.json for Windows..."
        $latestJson = @{
            version = "v$Version"
            notes = "See the release notes for v$Version"
            pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
            platforms = @{
                "windows-x86_64" = @{
                    signature = $signature
                    url = "https://github.com/moinulmoin/voicetypr/releases/download/$ReleaseTag/VoiceTypr_${Version}_x64-setup.exe"
                }
            }
        }
        
        $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
        Write-Success "Created latest.json for Windows"
    }
}

# If we skipped build, try to read signature from existing .sig file
if ($SkipBuild) {
    if (-not (Test-Path $OutputDir)) {
        Write-Error "Output directory not found: $OutputDir"
        exit 1
    }
    $sigPath = "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.sig"
    if (Test-Path $sigPath) {
        Write-Info "Reading signature from existing .sig file..."
        $signature = (Get-Content $sigPath -Raw).Trim()
        $signature = $signature -replace "`r`n", "" -replace "`n", ""
        Write-Success "Signature loaded from file"
    } else {
        Write-Error "No signature file found at $sigPath (required for auto-updates)"
        exit 1
    }
}

# Upload to GitHub
if (-not $SkipPublish) {
    Write-Step "Uploading to GitHub..."
    
    # Check if release exists
    gh release view $ReleaseTag 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Release $ReleaseTag not found. Run macOS release first to create the draft."
        exit 1
    }
    
    # Upload installer
    Write-Info "Uploading installer..."
    gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe" --clobber
    
    # Upload signature if it exists
    if (Test-Path "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.sig") {
        Write-Info "Uploading signature..."
        gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.sig" --clobber
    }
    
    # Upload latest.json
    if (Test-Path "$OutputDir\latest.json") {
        Write-Info "Uploading latest.json..."
        gh release upload $ReleaseTag "$OutputDir\latest.json" --clobber
    }
    
    Write-Success "Installer uploaded successfully!"
}

Write-Step "Done!"
Write-Info "Smart installer: VoiceTypr_${Version}_x64-setup.exe"
Write-Info "Direct downloads enabled - no ZIP required!"
Write-Info "Features:"
Write-Info "  • Auto-detects GPU capability"
Write-Info "  • Informs about GPU acceleration options"
Write-Info "  • Falls back to CPU if needed"
Write-Info "  • Single installer for all users!"
