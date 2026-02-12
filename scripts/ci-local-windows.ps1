param(
    [switch]$Full,
    [switch]$SkipInstall,
    [switch]$SkipVulkanCheck,
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

Matches .github/workflows/ci.yml (backend-windows) by default:
  - cargo check
  - cargo test

Usage:
  powershell -ExecutionPolicy Bypass -File .\scripts\ci-local-windows.ps1
  powershell -ExecutionPolicy Bypass -File .\scripts\ci-local-windows.ps1 -Full
  powershell -ExecutionPolicy Bypass -File .\scripts\ci-local-windows.ps1 -SkipVulkanCheck

Notes:
  - If Vulkan SDK is required for compilation, install it from https://vulkan.lunarg.com/sdk/home
"@
    exit 0
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

Write-Step "CI (Windows)"
Write-Info "Repo: $RepoRoot"

Require-Command cargo

Write-Info "cargo: $(cargo -V)"

if (-not $SkipVulkanCheck) {
    if ([string]::IsNullOrEmpty($env:VULKAN_SDK) -or -not (Test-Path $env:VULKAN_SDK)) {
        Write-ErrorMsg "VULKAN_SDK is not set (or points to a missing path). CI installs Vulkan SDK on windows-latest."
        Write-Info "Install Vulkan SDK from: https://vulkan.lunarg.com/sdk/home"
        Write-Info "Then open a new terminal and ensure VULKAN_SDK is set."
        exit 1
    }
    Write-Success "Vulkan SDK detected: $env:VULKAN_SDK"
} else {
    Write-Info "Skipping Vulkan SDK check (-SkipVulkanCheck)"
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

Push-Location src-tauri
try {
    Write-Step "cargo check (src-tauri)"
    cargo check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Step "cargo test (src-tauri)"
    cargo test
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}

Write-Success "Done."
