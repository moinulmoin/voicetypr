# Beads Watch - Keeps bv pages in sync with bd database
# Runs bd export before each bv regeneration to prevent drift

$interval = 30  # seconds between checks

Write-Host "Beads Watch starting..."
Write-Host "  Interval: ${interval}s"
Write-Host "  Press Ctrl+C to stop"
Write-Host ""

$lastHash = ""

while ($true) {
    # Get current issue count from database
    $dbCount = (bd list --all 2>$null | Measure-Object -Line).Lines
    $jsonlCount = (Get-Content .beads/issues.jsonl -ErrorAction SilentlyContinue | Measure-Object -Line).Lines

    # Check if sync needed
    if ($dbCount -ne $jsonlCount) {
        $timestamp = Get-Date -Format "HH:mm:ss"
        Write-Host "[$timestamp] Sync needed: DB has $dbCount, JSONL has $jsonlCount"

        # Export database to JSONL (force UTF-8 without BOM)
        $content = bd export | Out-String
        [System.IO.File]::WriteAllText(".beads/issues.jsonl", $content.Trim(), [System.Text.UTF8Encoding]::new($false))
        Write-Host "  -> Exported $dbCount issues to JSONL"

        # Regenerate bv pages
        bv --export-pages bv-site 2>&1 | Out-Null
        Write-Host "  -> Regenerated bv-site"
    }

    # Also check file hash for any changes
    $currentHash = (Get-FileHash .beads/issues.jsonl -Algorithm MD5 -ErrorAction SilentlyContinue).Hash
    if ($currentHash -ne $lastHash -and $lastHash -ne "") {
        $timestamp = Get-Date -Format "HH:mm:ss"
        Write-Host "[$timestamp] JSONL changed, regenerating..."
        bv --export-pages bv-site 2>&1 | Out-Null
        Write-Host "  -> Done"
    }
    $lastHash = $currentHash

    Start-Sleep -Seconds $interval
}
