# Beads Watch - Keeps bv pages in sync with bd database
# Compares DB content hash vs JSONL hash to detect ANY changes (not just count)

$interval = 30  # seconds between checks

Write-Host "Beads Watch starting..."
Write-Host "  Interval: ${interval}s"
Write-Host "  Press Ctrl+C to stop"
Write-Host ""

$lastDbHash = ""
$lastJsonlHash = ""

while ($true) {
    # Get current DB content hash (what SHOULD be in JSONL)
    $dbContent = bd export 2>$null | Out-String
    $dbContent = $dbContent.Trim()
    $dbHash = [System.BitConverter]::ToString(
        [System.Security.Cryptography.MD5]::Create().ComputeHash(
            [System.Text.Encoding]::UTF8.GetBytes($dbContent)
        )
    ).Replace("-", "")

    # Get current JSONL file hash
    $jsonlHash = (Get-FileHash .beads/issues.jsonl -Algorithm MD5 -ErrorAction SilentlyContinue).Hash

    # Check if DB content differs from JSONL file
    if ($dbHash -ne $jsonlHash) {
        $timestamp = Get-Date -Format "HH:mm:ss"
        $issueCount = ($dbContent -split "`n").Count
        Write-Host "[$timestamp] DB changed, syncing $issueCount issues..."

        # Write DB content to JSONL (force UTF-8 without BOM)
        [System.IO.File]::WriteAllText(".beads/issues.jsonl", $dbContent, [System.Text.UTF8Encoding]::new($false))
        Write-Host "  -> Exported to JSONL"

        # Regenerate bv pages
        bv --export-pages bv-site 2>&1 | Out-Null
        Write-Host "  -> Regenerated bv-site"

        # Update hash after sync
        $jsonlHash = $dbHash
    }

    # Also regenerate if JSONL changed externally (e.g., git pull)
    if ($jsonlHash -ne $lastJsonlHash -and $lastJsonlHash -ne "") {
        $timestamp = Get-Date -Format "HH:mm:ss"
        Write-Host "[$timestamp] JSONL changed externally, regenerating..."
        bv --export-pages bv-site 2>&1 | Out-Null
        Write-Host "  -> Done"
    }
    $lastJsonlHash = $jsonlHash

    Start-Sleep -Seconds $interval
}
