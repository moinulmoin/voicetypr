#!/bin/bash
# Beads Watch - Keeps bv pages in sync with bd database AND runs preview server
# Compares DB content hash vs JSONL hash to detect ANY changes

INTERVAL=5  # seconds between checks

echo "Beads Watch starting..."
echo "  Interval: ${INTERVAL}s"
echo ""

# Start the preview server in background
echo "Starting bv preview server..."
bv --preview-pages bv-site > /dev/null 2>&1 &
BV_PID=$!
sleep 2
echo "  -> Preview server running at http://127.0.0.1:9001 (PID: $BV_PID)"
echo ""
echo "Press Ctrl+C to stop both daemon and server"
echo ""

# Cleanup on exit
cleanup() {
    echo ""
    echo "Stopping preview server..."
    kill $BV_PID 2>/dev/null
    echo "Beads Watch stopped."
    exit 0
}
trap cleanup SIGINT SIGTERM

LAST_DB_HASH=""
LAST_JSONL_HASH=""

# Helper function to compute MD5 hash (cross-platform)
compute_md5() {
    if command -v md5sum &> /dev/null; then
        echo "$1" | md5sum | cut -d' ' -f1
    else
        echo "$1" | md5 -q
    fi
}

file_md5() {
    if command -v md5sum &> /dev/null; then
        md5sum "$1" 2>/dev/null | cut -d' ' -f1
    else
        md5 -q "$1" 2>/dev/null
    fi
}

while true; do
    # Get current DB content hash (what SHOULD be in JSONL)
    DB_CONTENT=$(bd export 2>/dev/null)
    DB_HASH=$(compute_md5 "$DB_CONTENT")

    # Get current JSONL file hash
    JSONL_HASH=$(file_md5 .beads/issues.jsonl)

    # Check if DB content differs from JSONL file
    if [ "$DB_HASH" != "$JSONL_HASH" ]; then
        TIMESTAMP=$(date "+%H:%M:%S")
        ISSUE_COUNT=$(echo "$DB_CONTENT" | wc -l | tr -d ' ')
        echo "[$TIMESTAMP] DB changed, syncing $ISSUE_COUNT issues..."

        # Write DB content to JSONL
        echo "$DB_CONTENT" > .beads/issues.jsonl
        echo "  -> Exported to JSONL"

        # Regenerate bv pages
        bv --export-pages bv-site > /dev/null 2>&1
        echo "  -> Regenerated bv-site"

        JSONL_HASH="$DB_HASH"
    fi

    # Also regenerate if JSONL changed externally (e.g., git pull)
    if [ -n "$LAST_JSONL_HASH" ] && [ "$JSONL_HASH" != "$LAST_JSONL_HASH" ]; then
        TIMESTAMP=$(date "+%H:%M:%S")
        echo "[$TIMESTAMP] JSONL changed externally, regenerating..."
        bv --export-pages bv-site > /dev/null 2>&1
        echo "  -> Done"
    fi
    LAST_JSONL_HASH="$JSONL_HASH"

    # Check if preview server is still running
    if ! kill -0 $BV_PID 2>/dev/null; then
        echo "Preview server stopped, restarting..."
        bv --preview-pages bv-site > /dev/null 2>&1 &
        BV_PID=$!
    fi

    sleep $INTERVAL
done
