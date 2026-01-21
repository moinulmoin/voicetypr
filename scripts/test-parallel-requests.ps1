# Test parallel transcription requests to remote server
# Usage: .\test-parallel-requests.ps1 [-Port 47842] [-NumRequests 5]

param(
    [int]$Port = 47842,
    [int]$NumRequests = 5,
    [string]$AudioFile = "$PSScriptRoot\..\tests\fixtures\audio-files\test-audio-16k.wav"
)

$serverUrl = "http://localhost:$Port/api/v1/transcribe"

# Check if audio file exists
if (-not (Test-Path $AudioFile)) {
    Write-Host "ERROR: Audio file not found: $AudioFile" -ForegroundColor Red
    exit 1
}

Write-Host "=== Parallel Transcription Request Test ===" -ForegroundColor Cyan
Write-Host "Server: $serverUrl"
Write-Host "Audio file: $AudioFile"
Write-Host "Number of parallel requests: $NumRequests"
Write-Host ""

# Read audio file
$audioBytes = [System.IO.File]::ReadAllBytes((Resolve-Path $AudioFile))
Write-Host "Audio file size: $($audioBytes.Length) bytes"
Write-Host ""

Write-Host "Launching $NumRequests parallel requests..." -ForegroundColor Yellow
Write-Host ""

$startTime = Get-Date

# Use runspaces for true parallel execution
$runspacePool = [runspacefactory]::CreateRunspacePool(1, $NumRequests)
$runspacePool.Open()

$runspaces = @()

for ($i = 1; $i -le $NumRequests; $i++) {
    $powershell = [powershell]::Create()
    $powershell.RunspacePool = $runspacePool

    [void]$powershell.AddScript({
        param($url, $audioBytes, $requestNum)

        $requestStart = Get-Date
        try {
            $response = Invoke-WebRequest -Uri $url `
                -Method POST `
                -ContentType "audio/wav" `
                -Body $audioBytes `
                -TimeoutSec 120 `
                -UseBasicParsing

            $requestEnd = Get-Date
            $duration = ($requestEnd - $requestStart).TotalMilliseconds

            $json = $response.Content | ConvertFrom-Json

            return [PSCustomObject]@{
                RequestNum = $requestNum
                Success = $true
                StatusCode = $response.StatusCode
                Duration = [math]::Round($duration, 0)
                Text = $json.text
                Model = $json.model
                Error = $null
            }
        }
        catch {
            $requestEnd = Get-Date
            $duration = ($requestEnd - $requestStart).TotalMilliseconds

            return [PSCustomObject]@{
                RequestNum = $requestNum
                Success = $false
                StatusCode = 0
                Duration = [math]::Round($duration, 0)
                Text = $null
                Model = $null
                Error = $_.Exception.Message
            }
        }
    })

    [void]$powershell.AddArgument($serverUrl)
    [void]$powershell.AddArgument($audioBytes)
    [void]$powershell.AddArgument($i)

    $runspaces += [PSCustomObject]@{
        PowerShell = $powershell
        Handle = $powershell.BeginInvoke()
        RequestNum = $i
    }
}

Write-Host "Waiting for all requests to complete..." -ForegroundColor Yellow
Write-Host ""

# Collect results
$results = @()
foreach ($runspace in $runspaces) {
    $result = $runspace.PowerShell.EndInvoke($runspace.Handle)
    $results += $result
    $runspace.PowerShell.Dispose()
}

$runspacePool.Close()
$runspacePool.Dispose()

$endTime = Get-Date
$totalDuration = ($endTime - $startTime).TotalMilliseconds

# Display results
Write-Host "=== Results ===" -ForegroundColor Cyan
Write-Host ""

$successCount = 0
$failCount = 0

foreach ($result in $results | Sort-Object RequestNum) {
    if ($result.Success) {
        $successCount++
        Write-Host "Request $($result.RequestNum): " -NoNewline
        Write-Host "SUCCESS" -ForegroundColor Green -NoNewline
        Write-Host " ($($result.Duration)ms) - '$($result.Text)'"
    }
    else {
        $failCount++
        Write-Host "Request $($result.RequestNum): " -NoNewline
        Write-Host "FAILED" -ForegroundColor Red -NoNewline
        Write-Host " ($($result.Duration)ms) - $($result.Error)"
    }
}

Write-Host ""
Write-Host "=== Summary ===" -ForegroundColor Cyan
Write-Host "Total time: $([math]::Round($totalDuration, 0))ms"
Write-Host "Successful: $successCount / $NumRequests"
Write-Host "Failed: $failCount / $NumRequests"

if ($failCount -eq 0) {
    Write-Host ""
    Write-Host "ALL REQUESTS COMPLETED SUCCESSFULLY" -ForegroundColor Green
    exit 0
}
else {
    Write-Host ""
    Write-Host "SOME REQUESTS FAILED" -ForegroundColor Red
    exit 1
}
