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

# Save original config at the start to avoid race condition
$originalConfig = Get-Content "src-tauri\tauri.conf.json" -Raw

# Create output directory
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir | Out-Null
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
    
    # Update tauri.conf.json to use smart installer hooks
    $config = Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
    # Create new nsis object with both properties
    $nsisConfig = @{
        installMode = "perMachine"
        installerHooks = "./windows/smart-installer-hooks.nsh"
    }
    $config.bundle.windows.nsis = $nsisConfig
    $config | ConvertTo-Json -Depth 10 | Set-Content "src-tauri\tauri.conf.json"
    
    # Clean to ensure fresh build
    cargo clean --manifest-path src-tauri\Cargo.toml
    
    # Build with Vulkan enabled by default
    Write-Info "Building with Vulkan support enabled..."
    pnpm tauri build
    
    if ($LASTEXITCODE -ne 0) {
        # Restore config
        $originalConfig | Set-Content "src-tauri\tauri.conf.json"
        Write-Error "Build failed!"
        exit 1
    }
    
    # Restore original config
    $originalConfig | Set-Content "src-tauri\tauri.conf.json"
    
    # Copy installer
    $installer = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" | Select-Object -First 1
    $installerPath = "$OutputDir\VoiceTypr_${Version}_x64-setup.exe"
    Copy-Item $installer.FullName $installerPath -Force
    Write-Success "Smart installer built successfully!"
    
    # Create update artifacts
    Write-Info "Creating update artifacts..."
    
    # Create .zip for updater
    $zipPath = "$installerPath.zip"
    Compress-Archive -Path $installerPath -DestinationPath $zipPath -Force
    Write-Success "Created update archive"
    
    # Sign if key available
    $keyPath = "$env:USERPROFILE\.tauri\voicetypr.key"
    if (Test-Path $keyPath) {
        Write-Info "Signing update artifact..."
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $keyPath
        
        & pnpm tauri signer sign -f $keyPath $zipPath
        
        if (Test-Path "$zipPath.sig") {
            Write-Success "Update artifact signed"
        } else {
            Write-Warning "Failed to sign update artifact"
        }
    } else {
        Write-Warning "No signing key found - updates won't have signatures"
    }
}

# Upload to GitHub
if (-not $SkipPublish) {
    Write-Step "Uploading to GitHub..."
    
    # Check if release exists
    try {
        gh release view $ReleaseTag | Out-Null
    } catch {
        Write-Error "Release $ReleaseTag not found. Create it first."
        exit 1
    }
    
    # Upload installer
    Write-Info "Uploading installer..."
    gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe" --clobber
    
    # Upload update artifacts if they exist
    if (Test-Path "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip") {
        gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip" --clobber
        if (Test-Path "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip.sig") {
            gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip.sig" --clobber
        }
    }
    
    Write-Success "Installer and update artifacts uploaded"
}

Write-Step "Done!"
Write-Info "Smart installer: VoiceTypr_${Version}_x64-setup.exe"
Write-Info "Features:"
Write-Info "  • Auto-detects GPU capability"
Write-Info "  • Informs about GPU acceleration options"
Write-Info "  • Falls back to CPU if needed"
Write-Info "  • Single installer for all users!"