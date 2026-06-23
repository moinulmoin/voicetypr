# Windows Release Script - one CPU-safe installer with optional Vulkan GPU sidecar

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
Windows Release Script

Builds one Windows installer:
- voicetypr.exe is CPU-safe and must not import vulkan-1.dll
- optional Vulkan acceleration ships as a sidecar process
- VC++ Runtime and Vulkan Runtime installers are bundled as resources
- updater/latest.json points to this single installer

Usage:
  .\scripts\release-windows.ps1                    # Build and upload installer
  .\scripts\release-windows.ps1 -SkipBuild         # Upload existing build
  .\scripts\release-windows.ps1 -SkipPublish       # Build only, don't upload
  .\scripts\release-windows.ps1 -Help              # Show this help

Requirements for building:
  - Vulkan SDK in VULKAN_SDK, used only to build/package the GPU sidecar
"@
    exit 0
}

if (-not $Version) {
    $packageJson = Get-Content "package.json" | ConvertFrom-Json
    $Version = $packageJson.version
}

Write-Step "Voicetypr Windows Release v$Version"

$ReleaseTag = "v$Version"
$OutputDir = "release-windows-$Version"
$InstallerName = "Voicetypr_${Version}_x64-setup.exe"

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
    Write-Step "Building CPU-safe app with bundled Vulkan sidecar"

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

    Write-Info "Building Whisper Vulkan sidecar..."
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    cargo build --manifest-path sidecar\whisper-vulkan\Cargo.toml --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Vulkan sidecar build failed"
        exit $LASTEXITCODE
    }

    New-Item -ItemType Directory -Path "sidecar\whisper-vulkan\dist" -Force | Out-Null
    Copy-Item "sidecar\whisper-vulkan\target\release\whisper-vulkan-sidecar.exe" `
        "sidecar\whisper-vulkan\dist\whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe" -Force

    Write-Info "Building Tauri installer..."
    pnpm tauri build --ci --config src-tauri/tauri.windows.conf.json
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed"
        exit $LASTEXITCODE
    }

    powershell -ExecutionPolicy Bypass -File .\src-tauri\windows\assert-no-vulkan-import.ps1 -ExePath "src-tauri\target\release\voicetypr.exe"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $installer = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        Write-Error "No installer found in src-tauri\target\release\bundle\nsis\"
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
Write-Info "Main app is CPU-safe; optional GPU acceleration is isolated in the bundled sidecar."
