# PowerShell script to run Rust tests on Windows with proper manifest
# This fixes the TaskDialogIndirect entry point not found error
# See: https://github.com/tauri-apps/tauri/issues/13419

param(
    [string]$TestFilter = "",
    [switch]$NoCapture,
    [switch]$IgnoredOnly
)

$ErrorActionPreference = "Stop"

# Create manifest file if it doesn't exist
$manifestPath = "$PSScriptRoot\test-manifest.xml"
$manifestContent = @"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"@

Set-Content -Path $manifestPath -Value $manifestContent -Encoding UTF8

# Build tests first
Write-Host "Building tests..." -ForegroundColor Cyan
$buildArgs = @("test", "--no-run")
if ($TestFilter) {
    $buildArgs += $TestFilter
}
& cargo $buildArgs

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

# Find test executables
$testExes = Get-ChildItem -Path "target\debug\deps" -Filter "voicetypr_lib-*.exe" |
    Where-Object { $_.Name -notmatch "\.d$" }

foreach ($exe in $testExes) {
    Write-Host "Embedding manifest into $($exe.Name)..." -ForegroundColor Yellow

    # Use mt.exe to embed manifest (from Windows SDK)
    $mtExe = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\mt.exe"
    if (-not (Test-Path $mtExe)) {
        # Try older SDK version
        $mtExe = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\mt.exe"
    }

    if (Test-Path $mtExe) {
        & $mtExe -manifest $manifestPath -outputresource:"$($exe.FullName);1" 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  Manifest embedded successfully" -ForegroundColor Green
        }
    } else {
        Write-Host "Warning: mt.exe not found in Windows SDK" -ForegroundColor Yellow
    }
}

# Run tests
Write-Host "Running tests..." -ForegroundColor Cyan
$testArgs = @("test")
if ($TestFilter) {
    $testArgs += $TestFilter
}
$testArgs += "--"
if ($IgnoredOnly) {
    $testArgs += "--ignored"
}
if ($NoCapture) {
    $testArgs += "--nocapture"
}

& cargo $testArgs
exit $LASTEXITCODE
