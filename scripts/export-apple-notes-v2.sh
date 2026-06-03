#!/usr/bin/env bash
#
# Export all notes from Apple Notes app (improved version)
# Creates individual text files in an export directory
#

set -euo pipefail

EXPORT_DIR="${1:-apple-notes-export}"
mkdir -p "$EXPORT_DIR"

echo "Exporting Apple Notes to: $EXPORT_DIR"
echo ""

# Get total count first
TOTAL=$(osascript -e 'tell application "Notes" to count notes')
echo "Found $TOTAL notes to export"
echo ""

# Export notes one at a time with progress tracking
for i in $(seq 1 "$TOTAL"); do
    echo -n "[$i/$TOTAL] Exporting note... "

    # Export single note with error handling
    if osascript <<EOF 2>/dev/null
tell application "Notes"
    try
        set currentNote to note $i
        set noteName to name of currentNote
        set noteBody to body of currentNote

        -- Clean filename
        set safeFilename to do shell script "echo " & quoted form of noteName & " | sed 's/[^a-zA-Z0-9]/-/g' | cut -c1-50"
        set filePath to "$PWD/$EXPORT_DIR/" & safeFilename & "-" & $i & ".txt"

        -- Ensure directory exists and write content (no metadata)
        do shell script "mkdir -p " & quoted form of "$PWD/$EXPORT_DIR" & " && echo " & quoted form of noteBody & " > " & quoted form of filePath

        return "OK"
    on error errMsg
        return "ERROR: " & errMsg
    end try
end tell
EOF
    then
        echo "✓"
    else
        echo "✗ (skipped)"
    fi
done

NOTE_COUNT=$(find "$EXPORT_DIR" -type f -name "*.txt" | wc -l | tr -d ' ')
echo ""
echo "✓ Exported $NOTE_COUNT notes to $EXPORT_DIR/"
echo ""
echo "Next step: Run ./scripts/import-notes.sh $EXPORT_DIR"
