# PowerShell script to run Rust tests with manifest embedding for Windows
# This fixes the TaskDialogIndirect / Common Controls v6 issue

param(
    [string]$TestFilter = "",
    [switch]$NoBuild = $false
)

$ErrorActionPreference = "Continue"

# Find Visual Studio installation
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vsWhere)) {
    Write-Error "vswhere.exe not found. Please install Visual Studio."
    exit 1
}

$vsPath = & $vsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsPath) {
    Write-Error "Visual Studio with VC Tools not found."
    exit 1
}

# Find vcvarsall.bat
$vcvarsall = Join-Path $vsPath "VC\Auxiliary\Build\vcvarsall.bat"
if (-not (Test-Path $vcvarsall)) {
    Write-Error "vcvarsall.bat not found at: $vcvarsall"
    exit 1
}

Write-Host "Using Visual Studio at: $vsPath" -ForegroundColor Cyan

# Build the test binary first (compile only, don't run)
if (-not $NoBuild) {
    Write-Host "Building test binary..." -ForegroundColor Yellow
    cargo test --no-run --lib 2>&1 | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed"
        exit 1
    }
}

# Find the test binary
$testBinaryPattern = "target\debug\deps\voicetypr_lib-*.exe"
$testBinaries = Get-ChildItem -Path $testBinaryPattern -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notmatch "\.d$" } |
    Sort-Object LastWriteTime -Descending

if ($testBinaries.Count -eq 0) {
    Write-Error "No test binary found matching: $testBinaryPattern"
    exit 1
}

$testBinary = $testBinaries[0].FullName
Write-Host "Found test binary: $testBinary" -ForegroundColor Green

# Path to our manifest
$manifestPath = Join-Path $PSScriptRoot "test.manifest"
if (-not (Test-Path $manifestPath)) {
    Write-Error "Manifest file not found at: $manifestPath"
    exit 1
}

# Embed manifest using mt.exe from VS Developer Command Prompt
Write-Host "Embedding manifest..." -ForegroundColor Yellow

$mtCommand = @"
call "$vcvarsall" x64 >nul 2>&1
mt.exe -manifest "$manifestPath" -outputresource:"$testBinary";1
"@

$tempBat = [System.IO.Path]::GetTempFileName() + ".bat"
Set-Content -Path $tempBat -Value $mtCommand -Encoding ASCII

$process = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", $tempBat -Wait -PassThru -NoNewWindow
Remove-Item $tempBat -ErrorAction SilentlyContinue

if ($process.ExitCode -ne 0) {
    Write-Error "Failed to embed manifest (exit code: $($process.ExitCode))"
    exit 1
}

Write-Host "Manifest embedded successfully" -ForegroundColor Green

# Run the tests
Write-Host "Running tests..." -ForegroundColor Yellow

if ($TestFilter) {
    & $testBinary $TestFilter
} else {
    & $testBinary
}

$testExitCode = $LASTEXITCODE
if ($testExitCode -eq 0) {
    Write-Host "Tests passed!" -ForegroundColor Green
} else {
    Write-Host "Tests failed with exit code: $testExitCode" -ForegroundColor Red
}

exit $testExitCode
