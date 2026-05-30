param(
    [switch]$Full,
    [switch]$SkipInstall,
    [switch]$GpuSidecar,
    [switch]$Help
)

function Write-Success($Message) { Write-Host "[OK] $Message" -ForegroundColor Green }
function Write-ErrorMsg($Message) { Write-Host "[ERROR] $Message" -ForegroundColor Red }
function Write-Info($Message) { Write-Host "[INFO] $Message" -ForegroundColor Cyan }
function Write-Step($Message) { Write-Host "`n==> $Message" -ForegroundColor Magenta }

function Require-Command($Command) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        Write-ErrorMsg "$Command not found in PATH"
        exit 1
    }
}

if ($Help) {
    Write-Host @"
Local Windows CI runner

Default checks:
  - cargo check
  - cargo test

Use -GpuSidecar to also compile the bundled optional Vulkan sidecar.
The main app still builds CPU-safe; the sidecar is the only Vulkan-linked binary.

Usage:
  powershell -ExecutionPolicy Bypass -File .\scripts\ci-local-windows.ps1
  powershell -ExecutionPolicy Bypass -File .\scripts\ci-local-windows.ps1 -Full
  powershell -ExecutionPolicy Bypass -File .\scripts\ci-local-windows.ps1 -GpuSidecar
"@
    exit 0
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

Write-Step "CI (Windows)"
Write-Info "Repo: $RepoRoot"

Require-Command cargo
Write-Info "cargo: $(cargo -V)"

if ($GpuSidecar) {
    if ([string]::IsNullOrEmpty($env:VULKAN_SDK) -or -not (Test-Path $env:VULKAN_SDK)) {
        Write-ErrorMsg "VULKAN_SDK is not set (or points to a missing path). It is required only for the optional GPU sidecar."
        Write-Info "Install Vulkan SDK from: https://vulkan.lunarg.com/sdk/home"
        exit 1
    }
    Write-Success "Vulkan SDK detected: $env:VULKAN_SDK"
}

if ($Full) {
    Require-Command node
    Require-Command pnpm

    Write-Info "node: $(node -v)"
    Write-Info "pnpm: $(pnpm -v)"

    if (-not $SkipInstall) {
        Write-Step "pnpm install --frozen-lockfile"
        pnpm install --frozen-lockfile
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } else {
        Write-Info "Skipping pnpm install (-SkipInstall)"
    }

    Write-Step "pnpm lint"
    pnpm lint
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Step "pnpm typecheck"
    pnpm typecheck
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Step "pnpm test run"
    pnpm test run
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if ($GpuSidecar) {
    Write-Step "cargo build Whisper Vulkan sidecar"
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    cargo build --manifest-path sidecar\whisper-vulkan\Cargo.toml --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Push-Location src-tauri
try {
    Write-Step "cargo check (src-tauri CPU-safe main app)"
    cargo check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Step "cargo test (src-tauri CPU-safe main app)"
    cargo test
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}

Write-Success "Done."
