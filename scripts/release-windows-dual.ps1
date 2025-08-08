# VoiceTypr Windows Dual Build Release Script
# This script builds both CPU and GPU versions for Windows
# It is designed to run AFTER the macOS release-separate.sh script
#
# Usage: .\scripts\release-windows-dual.ps1 [version]
# If version is not provided, it will be read from package.json

param(
    [string]$Version,
    [switch]$Help
)

# Colors for PowerShell output
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Color
}

function Write-Success { param([string]$Message) Write-ColorOutput "✓ $Message" "Green" }
function Write-Warning { param([string]$Message) Write-ColorOutput "⚠ $Message" "Yellow" }
function Write-Error { param([string]$Message) Write-ColorOutput "✗ $Message" "Red" }
function Write-Info { param([string]$Message) Write-ColorOutput "ℹ $Message" "Cyan" }
function Write-Step { param([string]$Message) Write-ColorOutput "🚀 $Message" "Magenta" }

# Show help
if ($Help) {
    Write-Host @"
VoiceTypr Windows Dual Build Release Script

This script builds both CPU and GPU (Vulkan) Windows installers for VoiceTypr.
It is designed to run AFTER the macOS release script has created the release.

Usage:
  .\scripts\release-windows-dual.ps1           # Use version from package.json
  .\scripts\release-windows-dual.ps1 1.6.0     # Use specific version
  .\scripts\release-windows-dual.ps1 -Help     # Show this help

Requirements:
- Node.js and pnpm installed
- Rust toolchain with windows targets
- GitHub CLI (gh) installed and authenticated
- TAURI_SIGNING_PRIVATE_KEY environment variable (optional, for update signatures)

The script will:
1. Read version from package.json (or use provided version)
2. Verify the GitHub release exists
3. Build CPU-only Windows NSIS installer
4. Build GPU (Vulkan) Windows NSIS installer
5. Create Tauri update artifacts for both versions
6. Download and update latest.json from GitHub release
7. Upload all Windows artifacts to the existing release

Build Variants:
- CPU Version: Works on all Windows systems (universal compatibility)
- GPU Version: Uses Vulkan acceleration (works with NVIDIA, AMD, Intel GPUs)

Environment Variables:
- TAURI_SIGNING_PRIVATE_KEY: Private key for signing update artifacts
- TAURI_SIGNING_PRIVATE_KEY_PASSWORD: Password for the private key (if needed)
- GITHUB_TOKEN: GitHub token for API access (gh CLI should handle this)
"@
    exit 0
}

Write-Step "Starting VoiceTypr Windows Dual Build Release Process"

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

# Function to build and process a variant
function Build-WindowsVariant {
    param(
        [string]$Variant,  # "cpu" or "gpu"
        [string]$Features  # Cargo features to enable
    )
    
    Write-Step "Building Windows $($Variant.ToUpper()) version..."
    
    # Clean previous build artifacts
    if (Test-Path "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis") {
        Remove-Item "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\*" -Force
    }
    
    try {
        Set-Location "src-tauri"
        
        # Build with appropriate features
        if ($Features) {
            Write-Info "Building with features: $Features"
            $buildOutput = cargo tauri build --target x86_64-pc-windows-msvc --features $Features 2>&1
        } else {
            Write-Info "Building CPU-only version (no additional features)"
            $buildOutput = cargo tauri build --target x86_64-pc-windows-msvc 2>&1
        }
        
        Set-Location ".."
        
        if ($LASTEXITCODE -ne 0) {
            Write-Error "Build failed for $Variant version. Output: $buildOutput"
            return $null
        }
        Write-Success "$($Variant.ToUpper()) build completed"
    } catch {
        Set-Location ".." -ErrorAction SilentlyContinue
        Write-Error "Failed to build $Variant version: $($_.Exception.Message)"
        return $null
    }
    
    # Find the built NSIS installer
    $NsisPath = Get-ChildItem -Path "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis" -Filter "*-setup.exe" | Select-Object -First 1
    if (-not $NsisPath) {
        Write-Error "NSIS installer not found for $Variant version"
        return $null
    }
    
    Write-Success "Found NSIS installer: $($NsisPath.Name)"
    
    # Rename based on variant
    $baseName = $NsisPath.Name -replace "_x64-setup\.exe$", ""
    if ($Variant -eq "gpu") {
        $newName = "${baseName}_x64-gpu-setup.exe"
    } else {
        $newName = "${baseName}_x64-setup.exe"
    }
    
    $NewNsisPath = Join-Path $OutputDir $newName
    Copy-Item $NsisPath.FullName $NewNsisPath
    Write-Success "Copied $Variant installer: $newName"
    
    # Create update artifacts
    Write-Info "Creating update artifacts for $Variant version..."
    
    # Create .nsis.zip for updater
    $NsisZipPath = "$NewNsisPath.zip"
    try {
        Compress-Archive -Path $NewNsisPath -DestinationPath $NsisZipPath -Force
        Write-Success "Created update archive: $(Split-Path $NsisZipPath -Leaf)"
    } catch {
        Write-Error "Failed to create NSIS zip: $($_.Exception.Message)"
        return $null
    }
    
    # Sign update artifacts if signing key is available
    $NsisZipSignature = "SIGNATURE_PLACEHOLDER"
    if ($HasSigningKey) {
        Write-Info "Signing $Variant update artifacts..."
        try {
            Set-Location "src-tauri"
            
            $signArgs = @("tauri", "signer", "sign")
            
            if ($env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
                $signArgs += @("-f", $env:TAURI_SIGNING_PRIVATE_KEY_PATH)
            }
            
            # No password for the key
            $signArgs += @("-p", "")
            $signArgs += $NsisZipPath
            
            $signOutput = & cargo @signArgs 2>&1
            Set-Location ".."
            
            if ($LASTEXITCODE -eq 0 -and (Test-Path "$NsisZipPath.sig")) {
                $NsisZipSignature = Get-Content "$NsisZipPath.sig" -Raw
                $NsisZipSignature = $NsisZipSignature.Trim()
                Write-Success "$Variant update artifact signed successfully"
            } else {
                Write-Warning "Failed to sign $Variant update artifact. Output: $signOutput"
            }
        } catch {
            Set-Location ".." -ErrorAction SilentlyContinue
            Write-Warning "Error during signing $Variant: $($_.Exception.Message)"
        }
    }
    
    return @{
        Installer = $NewNsisPath
        UpdateZip = $NsisZipPath
        Signature = $NsisZipSignature
        SignatureFile = if (Test-Path "$NsisZipPath.sig") { "$NsisZipPath.sig" } else { $null }
    }
}

# Build CPU version
Write-Step "Building CPU version (universal compatibility)..."
$cpuBuild = Build-WindowsVariant -Variant "cpu" -Features ""
if (-not $cpuBuild) {
    Write-Error "Failed to build CPU version"
    exit 1
}

# Check for Vulkan SDK before attempting GPU build
Write-Step "Checking for Vulkan SDK for GPU build..."
$CanBuildGPU = $false
$VulkanSDKFound = $false

# Check VULKAN_SDK environment variable
if ($env:VULKAN_SDK) {
    if (Test-Path $env:VULKAN_SDK) {
        Write-Success "Vulkan SDK found at: $env:VULKAN_SDK"
        $VulkanSDKFound = $true
        
        # Check for vulkan-1.lib
        $VulkanLib = Join-Path $env:VULKAN_SDK "Lib\vulkan-1.lib"
        if (Test-Path $VulkanLib) {
            Write-Success "vulkan-1.lib found"
            $CanBuildGPU = $true
        } else {
            Write-Warning "vulkan-1.lib not found at expected location: $VulkanLib"
        }
        
        # Optional: Check for vulkaninfo tool
        $VulkanInfo = Join-Path $env:VULKAN_SDK "Bin\vulkaninfo.exe"
        if (Test-Path $VulkanInfo) {
            Write-Info "vulkaninfo tool available for testing"
        }
    } else {
        Write-Warning "VULKAN_SDK environment variable set but path does not exist: $env:VULKAN_SDK"
    }
} else {
    Write-Warning "VULKAN_SDK environment variable not set"
}

# Alternative: Check common Vulkan SDK installation paths
if (-not $VulkanSDKFound) {
    $CommonPaths = @(
        "C:\VulkanSDK",
        "$env:ProgramFiles\VulkanSDK",
        "$env:LOCALAPPDATA\VulkanSDK"
    )
    
    foreach ($BasePath in $CommonPaths) {
        if (Test-Path $BasePath) {
            $SDKVersions = Get-ChildItem -Path $BasePath -Directory | Sort-Object Name -Descending
            if ($SDKVersions.Count -gt 0) {
                $LatestSDK = $SDKVersions[0].FullName
                Write-Info "Found Vulkan SDK at: $LatestSDK"
                Write-Warning "Please set VULKAN_SDK environment variable to: $LatestSDK"
                break
            }
        }
    }
}

# Build GPU version with Vulkan if SDK is available
if ($CanBuildGPU) {
    Write-Step "Building GPU version (Vulkan acceleration)..."
    $gpuBuild = Build-WindowsVariant -Variant "gpu" -Features "gpu-windows"
    if (-not $gpuBuild) {
        Write-Error "Failed to build GPU version"
        Write-Warning "Continuing with CPU-only release..."
        $gpuBuild = $null
    }
} else {
    Write-Warning "Skipping GPU build - Vulkan SDK not found or not properly configured"
    Write-Warning "To build GPU version:"
    Write-Warning "1. Download and install Vulkan SDK from: https://vulkan.lunarg.com/sdk/home"
    Write-Warning "2. Set VULKAN_SDK environment variable to the SDK installation path"
    Write-Warning "3. Restart PowerShell and run this script again"
    Write-Info "Continuing with CPU-only release..."
    $gpuBuild = $null
}

# Download and update latest.json
Write-Step "Updating latest.json with Windows platforms..."
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
    
    # Add Windows CPU platform
    $windowsCpuPlatform = @{
        signature = $cpuBuild.Signature
        url = "https://github.com/moinulmoin/voicetypr/releases/download/$ReleaseTag/$(Split-Path $cpuBuild.UpdateZip -Leaf)"
    }
    
    # Ensure platforms object exists
    if (-not $latestJson.platforms) {
        $latestJson | Add-Member -NotePropertyName "platforms" -NotePropertyValue @{}
    }
    
    # Add windows-x86_64 platform (CPU version as default)
    $latestJson.platforms | Add-Member -NotePropertyName "windows-x86_64" -NotePropertyValue $windowsCpuPlatform -Force
    
    # Note: GPU version would need a separate update channel or mechanism
    # For now, users will manually download the GPU version if they want it
    
    # Save updated JSON
    $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
    Write-Success "Added Windows platforms to latest.json"
    
} catch {
    Write-Error "Failed to update latest.json: $($_.Exception.Message)"
    exit 1
}

# Upload all Windows artifacts to release
Write-Step "Uploading Windows artifacts to GitHub release..."
try {
    # Upload CPU installer
    Write-Info "Uploading CPU installer..."
    gh release upload $ReleaseTag $cpuBuild.Installer --clobber
    Write-Success "Uploaded: $(Split-Path $cpuBuild.Installer -Leaf)"
    
    # Upload CPU update zip
    Write-Info "Uploading CPU update archive..."
    gh release upload $ReleaseTag $cpuBuild.UpdateZip --clobber
    Write-Success "Uploaded: $(Split-Path $cpuBuild.UpdateZip -Leaf)"
    
    # Upload CPU signature if it exists
    if ($cpuBuild.SignatureFile) {
        Write-Info "Uploading CPU signature..."
        gh release upload $ReleaseTag $cpuBuild.SignatureFile --clobber
        Write-Success "Uploaded: $(Split-Path $cpuBuild.SignatureFile -Leaf)"
    }
    
    # Upload GPU artifacts if build succeeded
    if ($gpuBuild) {
        # Upload GPU installer
        Write-Info "Uploading GPU installer..."
        gh release upload $ReleaseTag $gpuBuild.Installer --clobber
        Write-Success "Uploaded: $(Split-Path $gpuBuild.Installer -Leaf)"
        
        # Upload GPU update zip
        Write-Info "Uploading GPU update archive..."
        gh release upload $ReleaseTag $gpuBuild.UpdateZip --clobber
        Write-Success "Uploaded: $(Split-Path $gpuBuild.UpdateZip -Leaf)"
        
        # Upload GPU signature if it exists
        if ($gpuBuild.SignatureFile) {
            Write-Info "Uploading GPU signature..."
            gh release upload $ReleaseTag $gpuBuild.SignatureFile --clobber
            Write-Success "Uploaded: $(Split-Path $gpuBuild.SignatureFile -Leaf)"
        }
    } else {
        Write-Warning "GPU build was skipped - no GPU artifacts to upload"
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
Write-Step "Windows Dual Build Release Complete!"
Write-Success "Successfully created and uploaded Windows release artifacts for v$Version"
Write-Host ""
Write-Info "📦 Windows Release Artifacts:"
Write-Host ""
Write-Info "CPU Version (Universal Compatibility):" -ForegroundColor Cyan
Get-ChildItem $OutputDir -Filter "*_x64-setup*" | ForEach-Object {
    $size = if ($_.Length -gt 1MB) { "{0:N2} MB" -f ($_.Length / 1MB) } else { "{0:N2} KB" -f ($_.Length / 1KB) }
    Write-Host "   $($_.Name) ($size)" -ForegroundColor White
}
if ($gpuBuild) {
    Write-Host ""
    Write-Info "GPU Version (Vulkan Acceleration):" -ForegroundColor Cyan
    Get-ChildItem $OutputDir -Filter "*gpu*" | ForEach-Object {
        $size = if ($_.Length -gt 1MB) { "{0:N2} MB" -f ($_.Length / 1MB) } else { "{0:N2} KB" -f ($_.Length / 1KB) }
        Write-Host "   $($_.Name) ($size)" -ForegroundColor White
    }
}

Write-Host ""
Write-Info "🔗 Release URL: https://github.com/moinulmoin/voicetypr/releases/tag/$ReleaseTag"

if ($HasSigningKey) {
    Write-Success "✓ Update signatures generated - auto-updater ready"
} else {
    Write-Warning "⚠ No update signatures - auto-updater won't work"
}

Write-Host ""
Write-Info "📋 Build Information:"
Write-Host "• CPU Version: Works on ALL Windows 10/11 systems" -ForegroundColor Green
if ($gpuBuild) {
    Write-Host "• GPU Version: Requires Vulkan-compatible GPU (NVIDIA, AMD, Intel)" -ForegroundColor Yellow
} else {
    Write-Host "• GPU Version: Not built (Vulkan SDK not available)" -ForegroundColor Red
}
Write-Host ""
Write-Info "📋 Next Steps:"
Write-Host "1. Test the CPU installer on a Windows machine" -ForegroundColor Yellow
if ($gpuBuild) {
    Write-Host "2. Test the GPU installer on a Windows machine with NVIDIA/AMD/Intel GPU" -ForegroundColor Yellow
    Write-Host "3. Update release notes to explain the two versions" -ForegroundColor Yellow
    Write-Host "4. Add download instructions for users to choose the right version" -ForegroundColor Yellow
    Write-Host "5. Publish the release when ready" -ForegroundColor Yellow
} else {
    Write-Host "2. Update release notes (CPU-only for Windows)" -ForegroundColor Yellow
    Write-Host "3. Consider building GPU version later with Vulkan SDK installed" -ForegroundColor Yellow
    Write-Host "4. Publish the release when ready" -ForegroundColor Yellow
}

Write-Success "🎉 Windows dual build release process completed successfully!"