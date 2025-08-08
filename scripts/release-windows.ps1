# Simple Windows Release Script - Builds CPU and GPU versions

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

Builds two versions:
1. CPU version - works everywhere
2. GPU version - REQUIRES Vulkan (installer will force install it)

Usage:
  .\scripts\release-windows.ps1                    # Build and upload both
  .\scripts\release-windows.ps1 -SkipBuild         # Upload existing builds
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

# Build CPU version
if (-not $SkipBuild) {
    Write-Step "Building CPU version..."
    
    # CPU build uses default config (no hooks)
    pnpm tauri build
    if ($LASTEXITCODE -ne 0) {
        Write-Error "CPU build failed!"
        exit 1
    }
    
    # Copy CPU installer
    $cpuInstaller = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" | Select-Object -First 1
    $cpuPath = "$OutputDir\VoiceTypr_${Version}_x64-setup.exe"
    Copy-Item $cpuInstaller.FullName $cpuPath -Force
    Write-Success "CPU version built"
    
    # Create update artifacts for CPU version
    Write-Info "Creating CPU update artifacts..."
    
    # Create .zip for updater
    $cpuZipPath = "$cpuPath.zip"
    Compress-Archive -Path $cpuPath -DestinationPath $cpuZipPath -Force
    Write-Success "Created CPU update archive"
    
    # Sign if key available
    $keyPath = "$env:USERPROFILE\.tauri\voicetypr.key"
    if (Test-Path $keyPath) {
        Write-Info "Signing CPU update artifact..."
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $keyPath
        
        & pnpm tauri signer sign -f $keyPath $cpuZipPath
        
        if (Test-Path "$cpuZipPath.sig") {
            Write-Success "CPU update artifact signed"
        } else {
            Write-Warning "Failed to sign CPU update artifact"
        }
    } else {
        Write-Warning "No signing key found - CPU updates won't have signatures"
    }
}

# Build GPU version
if (-not $SkipBuild) {
    Write-Step "Building GPU version..."
    
    # Check for Vulkan SDK
    if (-not $env:VULKAN_SDK) {
        Write-Error "VULKAN_SDK not set! GPU build requires Vulkan SDK."
        Write-Info "Download from: https://vulkan.lunarg.com/sdk/home"
        exit 1
    }
    
    # Update tauri.conf.json to use GPU hooks
    $config = Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
    # Create new nsis object with both properties
    $nsisConfig = @{
        installMode = "perMachine"
        installerHooks = "./windows/gpu-installer-hooks.nsh"
    }
    $config.bundle.windows.nsis = $nsisConfig
    $config | ConvertTo-Json -Depth 10 | Set-Content "src-tauri\tauri.conf.json"
    
    # Clean to ensure fresh build
    cargo clean --manifest-path src-tauri\Cargo.toml
    
    # Build with GPU features using the correct Tauri v2 syntax
    Write-Info "Building GPU version with: pnpm tauri build -- --features gpu-windows"
    pnpm tauri build -- --features gpu-windows
    
    if ($LASTEXITCODE -ne 0) {
        # Restore config
        $originalConfig | Set-Content "src-tauri\tauri.conf.json"
        Write-Error "GPU build failed!"
        exit 1
    }
    
    # Restore original config
    $originalConfig | Set-Content "src-tauri\tauri.conf.json"
    
    # Copy GPU installer with different name
    $gpuInstaller = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" | Select-Object -First 1
    $gpuPath = "$OutputDir\VoiceTypr_${Version}_x64-gpu-setup.exe"
    Copy-Item $gpuInstaller.FullName $gpuPath -Force
    Write-Success "GPU version built"
    
    # Create update artifacts for GPU version
    Write-Info "Creating GPU update artifacts..."
    
    # Create .zip for updater
    $gpuZipPath = "$gpuPath.zip"
    Compress-Archive -Path $gpuPath -DestinationPath $gpuZipPath -Force
    Write-Success "Created GPU update archive"
    
    # Sign if key available
    $keyPath = "$env:USERPROFILE\.tauri\voicetypr.key"
    if (Test-Path $keyPath) {
        Write-Info "Signing GPU update artifact..."
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $keyPath
        
        & pnpm tauri signer sign -f $keyPath $gpuZipPath
        
        if (Test-Path "$gpuZipPath.sig") {
            Write-Success "GPU update artifact signed"
        } else {
            Write-Warning "Failed to sign GPU update artifact"
        }
    } else {
        Write-Warning "No signing key found - GPU updates won't have signatures"
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
    
    # Upload CPU version
    Write-Info "Uploading CPU version..."
    gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe" --clobber
    
    # Upload CPU update artifacts if they exist
    if (Test-Path "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip") {
        gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip" --clobber
        if (Test-Path "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip.sig") {
            gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-setup.exe.zip.sig" --clobber
        }
    }
    
    # Upload GPU version
    Write-Info "Uploading GPU version..."
    gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-gpu-setup.exe" --clobber
    
    # Upload GPU update artifacts
    if (Test-Path "$OutputDir\VoiceTypr_${Version}_x64-gpu-setup.exe.zip") {
        gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-gpu-setup.exe.zip" --clobber
        if (Test-Path "$OutputDir\VoiceTypr_${Version}_x64-gpu-setup.exe.zip.sig") {
            gh release upload $ReleaseTag "$OutputDir\VoiceTypr_${Version}_x64-gpu-setup.exe.zip.sig" --clobber
        }
    }
    
    Write-Success "All versions and update artifacts uploaded"
}

Write-Step "Done!"
Write-Info "CPU version: VoiceTypr_${Version}_x64-setup.exe"
Write-Info "GPU version: VoiceTypr_${Version}_x64-gpu-setup.exe (forces Vulkan install)"