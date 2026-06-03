#!/usr/bin/env bash
#
# Import notes from a directory of text files
#

set -euo pipefail

IMPORT_DIR="${1:?Usage: $0 <directory>}"
API_URL="${NOTE_API_URL:-http://localhost:9999}"
NOTE_TOKEN="${NOTE_TOKEN:-}"

if [[ ! -d "$IMPORT_DIR" ]]; then
    echo "Error: Directory not found: $IMPORT_DIR"
    exit 1
fi

# Create temp file for tracking
TMP_SUCCESS=$(mktemp)
TMP_FAILED=$(mktemp)
trap "rm -f $TMP_SUCCESS $TMP_FAILED" EXIT

# Count files
mapfile -t FILES < <(find "$IMPORT_DIR" -type f \( -name "*.txt" -o -name "*.md" \))
TOTAL=${#FILES[@]}

if [[ "$TOTAL" -eq 0 ]]; then
    echo "No .txt or .md files found in $IMPORT_DIR"
    exit 1
fi

echo "Importing $TOTAL notes from: $IMPORT_DIR"
echo "API: $API_URL"
echo ""

# Import each file
for file in "${FILES[@]}"; do
    echo -n "Importing: $(basename "$file")... "

    # Read file content
    CONTENT=$(cat "$file")

    # Clean content: remove HTML tags
    CONTENT_CLEAN=$(echo "$CONTENT" | sed -E 's/<[^>]+>//g' | sed 's/&nbsp;/ /g' | sed 's/&amp;/\&/g' | sed 's/&lt;/</g' | sed 's/&gt;/>/g')

    # Create JSON payload
    JSON=$(jq -n --arg content "$CONTENT_CLEAN" '{content: $content, source_type: "import"}')

    # Send to API
    HEADERS=(-H "Content-Type: application/json")
    if [[ -n "$NOTE_TOKEN" ]]; then
        HEADERS+=(-H "Authorization: Bearer $NOTE_TOKEN")
    fi

    if curl -s -f -X POST "$API_URL/api/notes" "${HEADERS[@]}" -d "$JSON" > /dev/null 2>&1; then
        echo "✓"
        echo "1" >> "$TMP_SUCCESS"
    else
        echo "✗ Failed"
        echo "1" >> "$TMP_FAILED"
    fi
done

SUCCESS=$(wc -l < "$TMP_SUCCESS" | tr -d ' ')
FAILED=$(wc -l < "$TMP_FAILED" | tr -d ' ')

echo ""
echo "Import complete:"
echo "  ✓ Success: $SUCCESS"
if [[ $FAILED -gt 0 ]]; then
    echo "  ✗ Failed: $FAILED"
fi
echo ""
echo "View your notes at: $API_URL"
