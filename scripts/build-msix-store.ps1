param(
    [switch]$SkipTauriBuild,
    [switch]$SkipGpuSidecarBuild,
    [switch]$NoPack,
    [string]$CertPath = "",
    [string]$CertPassword = "",
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"

function Write-Step($Message) { Write-Host "`n==> $Message" -ForegroundColor Magenta }
function Write-Info($Message) { Write-Host "[INFO] $Message" -ForegroundColor Cyan }
function Write-Success($Message) { Write-Host "[OK] $Message" -ForegroundColor Green }

function Require-Command($Command) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Command"
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "Store MSIX packaging must run on Windows."
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

Require-Command pnpm
Require-Command node
Require-Command cargo
Require-Command winapp

$TargetTriple = "x86_64-pc-windows-msvc"
$TargetDir = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    Join-Path $RepoRoot "src-tauri\target"
} else {
    $env:CARGO_TARGET_DIR
}

Write-Step "Preparing Windows sidecars"
pnpm run sidecar:ensure-ffmpeg

if (-not $SkipGpuSidecarBuild) {
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    cargo build --manifest-path sidecar/whisper-vulkan/Cargo.toml --release
    if ($LASTEXITCODE -ne 0) { throw "Whisper Vulkan sidecar build failed with exit code $LASTEXITCODE" }

    $SidecarOutDir = Join-Path $RepoRoot "sidecar\whisper-vulkan\dist"
    New-Item -ItemType Directory -Force -Path $SidecarOutDir | Out-Null
    $BuiltSidecar = Join-Path $TargetDir "release\whisper-vulkan-sidecar.exe"
    if (-not (Test-Path $BuiltSidecar)) { throw "Whisper Vulkan sidecar not found: $BuiltSidecar" }
    Copy-Item $BuiltSidecar (Join-Path $SidecarOutDir "whisper-vulkan-sidecar-$TargetTriple.exe") -Force
    Copy-Item $BuiltSidecar (Join-Path $SidecarOutDir "whisper-vulkan-sidecar.exe") -Force
}

if (-not $SkipTauriBuild) {
    Write-Step "Building Tauri Store binary"
    $env:VOICETYPR_DISTRIBUTION = "store_msix"
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    pnpm tauri build --no-bundle --ci --config src-tauri/tauri.windows.store.conf.json
    if ($LASTEXITCODE -ne 0) { throw "Tauri Store build failed with exit code $LASTEXITCODE" }
}

Write-Step "Staging MSIX layout"
$StageDir = Join-Path $RepoRoot "target\store-msix\stage"
$AssetsDir = Join-Path $StageDir "Assets"
Remove-Item $StageDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $AssetsDir | Out-Null

$MainExe = Join-Path $TargetDir "release\voicetypr.exe"
if (-not (Test-Path $MainExe)) { throw "Voicetypr release binary not found: $MainExe" }
Copy-Item $MainExe (Join-Path $StageDir "voicetypr.exe") -Force

$Sidecars = @(
    "sidecar\ffmpeg\dist\ffmpeg.exe",
    "sidecar\ffmpeg\dist\ffprobe.exe",
    "sidecar\whisper-vulkan\dist\whisper-vulkan-sidecar.exe",
    "sidecar\whisper-vulkan\dist\whisper-vulkan-sidecar-$TargetTriple.exe"
)

foreach ($RelativePath in $Sidecars) {
    $Source = Join-Path $RepoRoot $RelativePath
    if (-not (Test-Path $Source)) { throw "Required Store sidecar missing: $Source" }
    Copy-Item $Source (Join-Path $StageDir (Split-Path -Leaf $Source)) -Force
}

$IconCopies = @{
    "StoreLogo.png" = "src-tauri\icons\StoreLogo.png"
    "Square44x44Logo.png" = "src-tauri\icons\Square44x44Logo.png"
    "Square71x71Logo.png" = "src-tauri\icons\Square71x71Logo.png"
    "Square150x150Logo.png" = "src-tauri\icons\Square150x150Logo.png"
    "Square310x310Logo.png" = "src-tauri\icons\Square310x310Logo.png"
}

foreach ($Name in $IconCopies.Keys) {
    $Source = Join-Path $RepoRoot $IconCopies[$Name]
    if (-not (Test-Path $Source)) { throw "Required MSIX asset missing: $Source" }
    Copy-Item $Source (Join-Path $AssetsDir $Name) -Force
}

$PackageVersion = node -p "require('./package.json').version + '.0'"
$ManifestSource = Join-Path $RepoRoot "src-tauri\msix\Package.appxmanifest"
$ManifestDest = Join-Path $StageDir "Package.appxmanifest"
$Manifest = Get-Content $ManifestSource -Raw
$Manifest = [regex]::Replace(
    $Manifest,
    '(?m)^(\s*Version=")[^"]+(")',
    ('${1}' + $PackageVersion + '${2}')
)
if ($Manifest -notmatch ('(?m)^\s*Version="' + [regex]::Escape($PackageVersion) + '"')) {
    throw "Failed to set MSIX package identity version to $PackageVersion."
}
if ($Manifest -notmatch 'MinVersion="10\.0\.19041\.0"') {
    throw "MSIX TargetDeviceFamily MinVersion must remain 10.0.19041.0 for Partner Center."
}
$Utf8NoBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
[System.IO.File]::WriteAllText($ManifestDest, $Manifest, $Utf8NoBom)

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $RepoRoot "target\store-msix\Voicetypr_${PackageVersion}_x64.msix"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = Join-Path $RepoRoot $OutputPath
}

Write-Success "MSIX stage ready: $StageDir"

if ($NoPack) {
    Write-Info "Skipping winapp pack because -NoPack was specified."
    exit 0
}

Write-Step "Packing MSIX"
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutputPath) | Out-Null

$PackArgs = @(
    "pack",
    $StageDir,
    "--manifest",
    $ManifestDest,
    "--output",
    $OutputPath,
    "--executable",
    "voicetypr.exe"
)

if ([string]::IsNullOrWhiteSpace($CertPath)) {
    Write-Info "Packing with a generated development certificate for CI/local install testing."
    $PackArgs += "--generate-cert"
    $PackArgs += "--install-cert"
} else {
    $ResolvedCertPath = if ([System.IO.Path]::IsPathRooted($CertPath)) {
        $CertPath
    } else {
        Join-Path $RepoRoot $CertPath
    }

    if (-not (Test-Path $ResolvedCertPath)) {
        throw "MSIX certificate not found: $ResolvedCertPath"
    }

    $PackArgs += "--cert"
    $PackArgs += $ResolvedCertPath

    if (-not [string]::IsNullOrWhiteSpace($CertPassword)) {
        $PackArgs += "--cert-password"
        $PackArgs += $CertPassword
    }
}

& winapp @PackArgs
if ($LASTEXITCODE -ne 0) { throw "winapp pack failed with exit code $LASTEXITCODE" }

Write-Success "MSIX package created: $OutputPath"
