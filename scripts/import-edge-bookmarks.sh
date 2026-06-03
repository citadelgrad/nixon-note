#!/usr/bin/env bash
set -euo pipefail

# Microsoft Edge bookmarks import script for nixonnote
# Captures all Edge bookmarks with metadata as notes

# Configuration
API_URL="${NOTE_API_URL:-http://localhost:9999}"
NOTE_TOKEN="${NOTE_TOKEN:-}"
EDGE_BOOKMARKS="$HOME/Library/Application Support/Microsoft Edge/Default/Bookmarks"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check dependencies
for cmd in jq curl; do
    if ! command -v "$cmd" &> /dev/null; then
        echo -e "${RED}Error: $cmd is required but not installed${NC}" >&2
        exit 1
    fi
done

echo "🌐 Microsoft Edge Bookmarks Import"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check if bookmarks file exists
if [ ! -f "$EDGE_BOOKMARKS" ]; then
    echo -e "${RED}Error: Edge bookmarks file not found at:${NC}"
    echo "$EDGE_BOOKMARKS"
    echo ""
    echo "Alternative locations to check:"
    echo "  ~/Library/Application Support/BraveSoftware/Brave-Browser/Default/Bookmarks"
    echo "  ~/Library/Application Support/Google/Chrome/Default/Bookmarks"
    exit 1
fi

echo "Reading bookmarks from: $EDGE_BOOKMARKS"
echo ""

# Recursive function to extract bookmarks from nested folders
extract_bookmarks() {
    local json="$1"
    local folder_path="${2:-}"

    # Get type
    local type=$(echo "$json" | jq -r '.type')

    if [ "$type" = "url" ]; then
        # Extract bookmark fields
        local name=$(echo "$json" | jq -r '.name')
        local url=$(echo "$json" | jq -r '.url')
        local date_added=$(echo "$json" | jq -r '.date_added // ""')

        # Build markdown content with folder context
        local content="# $name"
        if [ -n "$folder_path" ]; then
            content="$content

**Folder:** $folder_path"
        fi
        content="$content

**URL:** $url"

        # Output as JSON note object with tags
        jq -n \
            --arg content "$content" \
            --arg source "bookmark" \
            --arg url "$url" \
            "{
                content: \$content,
                source_type: \$source,
                source_url: \$url,
                tags: [\"hidden\", \"bookmark\"]
            }"
    elif [ "$type" = "folder" ]; then
        # Recurse into folder
        local folder_name=$(echo "$json" | jq -r '.name')
        local new_path="$folder_path"
        if [ -n "$new_path" ]; then
            new_path="$new_path / $folder_name"
        else
            new_path="$folder_name"
        fi

        # Process all children
        echo "$json" | jq -c '.children[]?' | while read -r child; do
            extract_bookmarks "$child" "$new_path"
        done
    fi
}

# Extract all bookmarks from all root folders
notes_json=$(jq -c '.roots | to_entries[] | .value' "$EDGE_BOOKMARKS" | \
    while read -r root; do
        extract_bookmarks "$root"
    done | jq -s '.')

# Check if we got any bookmarks
bookmark_count=$(echo "$notes_json" | jq 'length')
if [ "$bookmark_count" -eq 0 ]; then
    echo -e "${YELLOW}No bookmarks found in Edge${NC}"
    exit 0
fi

echo "Found $bookmark_count bookmarks"
echo ""

# Build batch request
batch_request=$(jq -n --argjson notes "$notes_json" '{ notes: $notes }')

# Send batch request
echo "Importing to nixonnote..."
response=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/notes/batch" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $NOTE_TOKEN" \
    -d "$batch_request")

http_code=$(echo "$response" | tail -n1)
response_body=$(echo "$response" | sed '$d')

if [ "$http_code" = "201" ]; then
    # Parse response
    note_ids=$(echo "$response_body" | jq -r '.note_ids | length')
    failed=$(echo "$response_body" | jq -r '.failed_count')

    echo -e "${GREEN}✓ Successfully imported $note_ids bookmarks${NC}"
    if [ "$failed" -gt 0 ]; then
        echo -e "${YELLOW}⚠ $failed bookmarks failed to import${NC}"
    fi

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Import complete! Query your bookmarks:"
    echo "  curl \"$API_URL/api/notes?q=bookmark\""
else
    echo -e "${RED}✗ Import failed (HTTP $http_code)${NC}"
    echo "$response_body" | jq '.' 2>/dev/null || echo "$response_body"
    exit 1
fi
