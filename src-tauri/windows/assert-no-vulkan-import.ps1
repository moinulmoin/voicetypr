# Fail if a Windows PE executable imports vulkan-1.dll (CPU build guard).
param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $ExePath)) {
    throw "Executable not found: $ExePath"
}

function Get-DumpBinPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) {
        return $null
    }

    $dumpbin = & $vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find '**\Hostx64\x64\dumpbin.exe' 2>$null |
        Select-Object -First 1

    if ([string]::IsNullOrWhiteSpace($dumpbin)) {
        return $null
    }

    return $dumpbin
}

$dumpbin = Get-DumpBinPath
if (-not $dumpbin) {
    throw 'dumpbin.exe not found (install Visual Studio C++ build tools on this runner)'
}

$imports = & $dumpbin /imports $ExePath 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "dumpbin failed for $ExePath (exit $LASTEXITCODE): $imports"
}

$matched = @($imports | Where-Object { $_ -match '(?i)^\s*vulkan-1\.dll\s*$' })
if ($matched.Count -gt 0) {
    Write-Error "CPU build must not import vulkan-1.dll, but $($matched.Count) import(s) were found in: $ExePath"
    exit 1
}

Write-Host "OK: $ExePath does not import vulkan-1.dll"
