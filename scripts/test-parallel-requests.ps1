# Test parallel transcription requests to remote server
# Usage: .\test-parallel-requests.ps1 [-Port 47842] [-NumRequests 5]

param(
    [int]$Port = 47842,
    [int]$NumRequests = 5,
    [string]$AudioFile = "$PSScriptRoot\..\tests\fixtures\audio-files\test-audio.wav"
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
$audioBytes = [System.IO.File]::ReadAllBytes($AudioFile)
Write-Host "Audio file size: $($audioBytes.Length) bytes"
Write-Host ""

# Create jobs for parallel requests
$jobs = @()
$startTime = Get-Date

Write-Host "Launching $NumRequests parallel requests..." -ForegroundColor Yellow
Write-Host ""

for ($i = 1; $i -le $NumRequests; $i++) {
    $jobs += Start-Job -ScriptBlock {
        param($url, $audioBytes, $requestNum)

        $requestStart = Get-Date
        try {
            $response = Invoke-WebRequest -Uri $url `
                -Method POST `
                -ContentType "audio/wav" `
                -Body $audioBytes `
                -TimeoutSec 120

            $requestEnd = Get-Date
            $duration = ($requestEnd - $requestStart).TotalMilliseconds

            $json = $response.Content | ConvertFrom-Json

            return @{
                RequestNum = $requestNum
                Success = $true
                Status = $response.StatusCode
                Duration = [math]::Round($duration, 0)
                Text = $json.text
                Model = $json.model
            }
        }
        catch {
            $requestEnd = Get-Date
            $duration = ($requestEnd - $requestStart).TotalMilliseconds

            return @{
                RequestNum = $requestNum
                Success = $false
                Status = $_.Exception.Response.StatusCode
                Duration = [math]::Round($duration, 0)
                Error = $_.Exception.Message
            }
        }
    } -ArgumentList $serverUrl, $audioBytes, $i
}

Write-Host "Waiting for all requests to complete..." -ForegroundColor Yellow
Write-Host ""

# Wait for all jobs and collect results
$results = @()
foreach ($job in $jobs) {
    $result = Receive-Job -Job $job -Wait
    $results += $result
    Remove-Job -Job $job
}

$endTime = Get-Date
$totalDuration = ($endTime - $startTime).TotalMilliseconds

# Display results
Write-Host "=== Results ===" -ForegroundColor Cyan
Write-Host ""

$successCount = 0
$failCount = 0

foreach ($result in $results | Sort-Object { $_.RequestNum }) {
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
