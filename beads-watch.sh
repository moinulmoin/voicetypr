#!/bin/bash
# Beads Watch - Keeps bv pages in sync with bd database
# Works on macOS and Linux

INTERVAL=30  # seconds between checks

echo "Beads Watch starting..."
echo "  Interval: ${INTERVAL}s"
echo "  Press Ctrl+C to stop"
echo ""

LAST_HASH=""

while true; do
    # Get current issue count from database
    DB_COUNT=$(bd list --all 2>/dev/null | wc -l | tr -d ' ')
    JSONL_COUNT=$(wc -l < .beads/issues.jsonl 2>/dev/null | tr -d ' ')

    # Check if sync needed
    if [ "$DB_COUNT" != "$JSONL_COUNT" ]; then
        TIMESTAMP=$(date "+%H:%M:%S")
        echo "[$TIMESTAMP] Sync needed: DB has $DB_COUNT, JSONL has $JSONL_COUNT"

        # Export database to JSONL
        bd export > .beads/issues.jsonl
        echo "  -> Exported $DB_COUNT issues to JSONL"

        # Regenerate bv pages
        bv --export-pages bv-site > /dev/null 2>&1
        echo "  -> Regenerated bv-site"
    fi

    # Also check file hash for any changes
    if command -v md5sum &> /dev/null; then
        CURRENT_HASH=$(md5sum .beads/issues.jsonl 2>/dev/null | cut -d' ' -f1)
    else
        CURRENT_HASH=$(md5 -q .beads/issues.jsonl 2>/dev/null)
    fi

    if [ -n "$LAST_HASH" ] && [ "$CURRENT_HASH" != "$LAST_HASH" ]; then
        TIMESTAMP=$(date "+%H:%M:%S")
        echo "[$TIMESTAMP] JSONL changed, regenerating..."
        bv --export-pages bv-site > /dev/null 2>&1
        echo "  -> Done"
    fi
    LAST_HASH="$CURRENT_HASH"

    sleep $INTERVAL
done
