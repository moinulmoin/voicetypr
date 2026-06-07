# Windows x86_64 release script - one CPU-safe installer with optional Vulkan GPU sidecar.
# Uses src-tauri/tauri.windows.conf.json, which bundles the x64 Vulkan sidecar only.
# Windows ARM64 builds stay CPU-only and must not use this config or sidecar path.

param(
    [string]$Version,
    [switch]$Help,
    [switch]$SkipBuild,
    [switch]$SkipPublish
)

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
Windows x86_64 Release Script

Builds one Windows x64 NSIS installer:
- voicetypr.exe is CPU-safe and must not import vulkan-1.dll
- optional Vulkan acceleration ships as an x86_64 sidecar process
- VC++ Runtime and Vulkan Runtime installers are bundled as resources
- updater/latest.json points to this single installer

This script and src-tauri/tauri.windows.conf.json are for x86_64 Windows only.
Windows ARM64 builds are CPU-only and must not require the x64 Vulkan sidecar.

Usage:
  .\scripts\release-windows.ps1                    # Build and upload installer
  .\scripts\release-windows.ps1 -SkipBuild         # Upload existing build
  .\scripts\release-windows.ps1 -SkipPublish       # Build only, don't upload
  .\scripts\release-windows.ps1 -Help              # Show this help

Requirements for building:
  - Vulkan SDK in VULKAN_SDK, used only to build/package the x64 GPU sidecar
  - Optional CARGO_TARGET_DIR for short build paths (honored for sidecar and main app)
"@
    exit 0
}

if (-not $Version) {
    $packageJson = Get-Content "package.json" | ConvertFrom-Json
    $Version = $packageJson.version
}

Write-Step "VoiceTypr Windows x86_64 Release v$Version"

$ReleaseTag = "v$Version"
$OutputDir = "release-windows-$Version"
$InstallerName = "VoiceTypr_${Version}_x64-setup.exe"

if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

Require-Command cargo
Require-Command pnpm
Require-File "package.json"

if (-not $SkipPublish) {
    Require-Command gh
}

if (-not $SkipBuild) {
    Write-Step "Building x86_64 CPU-safe app with bundled Vulkan sidecar"

    $AppTargetDir = $env:CARGO_TARGET_DIR
    if ([string]::IsNullOrWhiteSpace($AppTargetDir)) {
        $AppTargetDir = "src-tauri\target"
    }

    if ([string]::IsNullOrEmpty($env:VULKAN_SDK) -or -not (Test-Path $env:VULKAN_SDK)) {
        Write-Error "VULKAN_SDK not set. Install Vulkan SDK to build the optional GPU sidecar."
        Write-Info "Download from: https://vulkan.lunarg.com/sdk/home"
        exit 1
    }

    cargo clean --manifest-path src-tauri\Cargo.toml
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $runtimeDir = "src-tauri\windows\resources"
    New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null

    Write-Info "Downloading Visual C++ Runtime installer..."
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vc_redist.x64.exe" -OutFile "$runtimeDir\vc_redist.x64.exe"

    Write-Info "Downloading Vulkan Runtime installer..."
    $vulkanVersion = $env:VULKAN_RUNTIME_VERSION
    if ([string]::IsNullOrWhiteSpace($vulkanVersion)) {
        $vulkanVersion = $env:VULKAN_VERSION
    }
    if ([string]::IsNullOrWhiteSpace($vulkanVersion)) {
        $vulkanVersion = Split-Path -Leaf $env:VULKAN_SDK
    }
    if ([string]::IsNullOrWhiteSpace($vulkanVersion)) {
        Write-Error "Cannot determine Vulkan version. Set VULKAN_RUNTIME_VERSION, VULKAN_VERSION, or VULKAN_SDK."
        exit 1
    }
    $vulkanRuntimeUrl = "https://sdk.lunarg.com/sdk/download/$vulkanVersion/windows/VulkanRT-$vulkanVersion-Installer.exe"
    Invoke-WebRequest -Uri $vulkanRuntimeUrl -OutFile "$runtimeDir\VulkanRT-Installer.exe"

    Write-Info "Building Whisper Vulkan sidecar (x86_64 only)..."
    $SidecarTargetDir = $env:CARGO_TARGET_DIR
    if ([string]::IsNullOrWhiteSpace($SidecarTargetDir)) {
        $SidecarTargetDir = "sidecar\whisper-vulkan\target"
    }

    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    cargo build --manifest-path sidecar\whisper-vulkan\Cargo.toml --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Vulkan sidecar build failed"
        exit $LASTEXITCODE
    }

    $SidecarExe = Join-Path $SidecarTargetDir "release\whisper-vulkan-sidecar.exe"
    if (-not (Test-Path $SidecarExe)) {
        Write-Error "Whisper Vulkan sidecar binary not found after build: $SidecarExe"
        exit 1
    }

    $SidecarDist = "sidecar\whisper-vulkan\dist\whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe"
    New-Item -ItemType Directory -Path "sidecar\whisper-vulkan\dist" -Force | Out-Null
    Copy-Item $SidecarExe $SidecarDist -Force

    Write-Info "Building Tauri x86_64 installer..."
    pnpm tauri build --ci --config src-tauri/tauri.windows.conf.json
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed"
        exit $LASTEXITCODE
    }

    $MainExe = Join-Path $AppTargetDir "release\voicetypr.exe"
    powershell -ExecutionPolicy Bypass -File .\src-tauri\windows\assert-no-vulkan-import.ps1 -ExePath $MainExe
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $InstallerDir = Join-Path $AppTargetDir "release\bundle\nsis"
    $installer = Get-ChildItem "$InstallerDir\*.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        Write-Error "No installer found in $InstallerDir\"
        exit 1
    }

    $installerPath = "$OutputDir\$InstallerName"
    Copy-Item $installer.FullName $installerPath -Force
    Write-Success "Installer built: $installerPath"

    $keyPath = "$env:USERPROFILE\.tauri\voicetypr.key"
    $signature = ""
    if (Test-Path $keyPath) {
        Write-Info "Signing installer for updates..."
        if (-not [string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
            & pnpm tauri signer sign -f $keyPath -p $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD $installerPath
        } else {
            & pnpm tauri signer sign -f $keyPath --password= $installerPath
        }
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        if (Test-Path "$installerPath.sig") {
            $signature = (Get-Content "$installerPath.sig" -Raw).Trim() -replace "`r`n", "" -replace "`n", ""
            Write-Success "Installer signed"
        } else {
            Write-Error "Failed to sign installer (missing .sig file)"
            exit 1
        }
    } else {
        Write-Error "No signing key found at $keyPath (required for auto-updates)"
        exit 1
    }

    Write-Info "Updating latest.json with Windows platform..."
    $latestJsonPath = "$OutputDir\latest.json"
    try {
        gh release download $ReleaseTag -p "latest.json" -D $OutputDir --clobber 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path $latestJsonPath)) { Write-Success "Downloaded existing latest.json" }
    } catch {
        Write-Info "No existing latest.json downloaded: $_"
    }

    if (Test-Path $latestJsonPath) {
        $latestJson = Get-Content $latestJsonPath -Raw | ConvertFrom-Json
        if (-not $latestJson.platforms) {
            $latestJson | Add-Member -NotePropertyName "platforms" -NotePropertyValue @{} -Force
        }
        $windowsPlatform = @{
            signature = $signature
            url = "https://github.com/moinulmoin/voicetypr/releases/download/$ReleaseTag/$InstallerName"
        }
        $latestJson.platforms | Add-Member -NotePropertyName "windows-x86_64" -NotePropertyValue $windowsPlatform -Force
        $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
    } else {
        $latestJson = @{
            version = "v$Version"
            notes = "See the release notes for v$Version"
            pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
            platforms = @{
                "windows-x86_64" = @{
                    signature = $signature
                    url = "https://github.com/moinulmoin/voicetypr/releases/download/$ReleaseTag/$InstallerName"
                }
            }
        }
        $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
    }
}

if ($SkipBuild) {
    if (-not (Test-Path $OutputDir)) {
        Write-Error "Output directory not found: $OutputDir"
        exit 1
    }
    if (-not (Test-Path "$OutputDir\$InstallerName.sig")) {
        Write-Error "No signature file found at $OutputDir\$InstallerName.sig"
        exit 1
    }
}

if (-not $SkipPublish) {
    Write-Step "Uploading to GitHub"
    gh release view $ReleaseTag 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Release $ReleaseTag not found. Run macOS release first to create the draft."
        exit 1
    }

    gh release upload $ReleaseTag "$OutputDir\$InstallerName" --clobber
    gh release upload $ReleaseTag "$OutputDir\$InstallerName.sig" --clobber
    if (Test-Path "$OutputDir\latest.json") {
        gh release upload $ReleaseTag "$OutputDir\latest.json" --clobber
    }
    Write-Success "Installer uploaded successfully"
}

Write-Step "Done"
Write-Info "Installer: $InstallerName"
Write-Info "Main app is CPU-safe; optional x86_64 GPU acceleration is isolated in the bundled sidecar."
