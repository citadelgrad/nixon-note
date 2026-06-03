#!/usr/bin/env bash
#
# Export all notes from Apple Notes app
# Creates individual text files in an export directory
#

set -euo pipefail

EXPORT_DIR="${1:-apple-notes-export}"
mkdir -p "$EXPORT_DIR"

echo "Exporting Apple Notes to: $EXPORT_DIR"
echo ""

# Use osascript to export notes
osascript <<EOF
tell application "Notes"
    set exportFolder to POSIX file "$PWD/$EXPORT_DIR" as text
    set allNotes to every note
    set noteCount to count of allNotes

    repeat with i from 1 to noteCount
        set currentNote to item i of allNotes
        set noteName to name of currentNote
        set noteBody to body of currentNote
        set noteCreated to creation date of currentNote
        set noteModified to modification date of currentNote

        -- Clean filename
        set safeFilename to do shell script "echo " & quoted form of noteName & " | sed 's/[^a-zA-Z0-9]/-/g' | cut -c1-50"
        set filePath to "$PWD/$EXPORT_DIR/" & safeFilename & "-" & i & ".txt"

        -- Write metadata and content
        set fileContent to "Title: " & noteName & "
Created: " & noteCreated & "
Modified: " & noteModified & "

" & noteBody

        do shell script "echo " & quoted form of fileContent & " > " & quoted form of filePath
    end repeat

    return noteCount
end tell
EOF

NOTE_COUNT=$(ls -1 "$EXPORT_DIR" | wc -l | tr -d ' ')
echo ""
echo "✓ Exported $NOTE_COUNT notes to $EXPORT_DIR/"
echo ""
echo "Next step: Run ./scripts/import-notes.sh $EXPORT_DIR"
