#!/usr/bin/env bash
set -euo pipefail

# Homebrew packages import script for nixonnote
# Captures all installed Homebrew packages with metadata as notes

# Configuration
API_URL="${NOTE_API_URL:-http://localhost:9999}"
NOTE_TOKEN="${NOTE_TOKEN:-}"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check dependencies
for cmd in brew jq curl; do
    if ! command -v "$cmd" &> /dev/null; then
        echo -e "${RED}Error: $cmd is required but not installed${NC}" >&2
        exit 1
    fi
done

echo "🍺 Homebrew Package Import"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Get all packages
all_packages=$(brew list --formula -1)
total_packages=$(echo "$all_packages" | wc -l | tr -d ' ')
echo "Found $total_packages Homebrew packages"
echo ""

# Collect notes
notes=()
echo "Fetching package metadata..."

while IFS= read -r pkg; do
    echo -n "."

    # Get package info
    info=$(brew info --json=v2 "$pkg" 2>/dev/null | jq -r ".formulae[0]")

    if [ "$info" = "null" ] || [ -z "$info" ]; then
        continue
    fi

    name=$(echo "$info" | jq -r ".name // \"$pkg\"")
    desc=$(echo "$info" | jq -r ".desc // \"No description\"")
    homepage=$(echo "$info" | jq -r ".homepage // \"\"")
    version=$(echo "$info" | jq -r ".versions.stable // \"unknown\"")

    # Build markdown content
    content="# $name

**Version:** $version
**Description:** $desc"

    if [ -n "$homepage" ]; then
        content="$content
**Homepage:** $homepage"
    fi

    # Create note object with tags
    note=$(jq -n \
        --arg content "$content" \
        --arg source "homebrew" \
        --arg url "$pkg" \
        '{
            content: $content,
            source_type: $source,
            source_url: $url,
            tags: ["hidden", "tool"]
        }')

    notes+=("$note")
done <<< "$all_packages"

echo ""
echo "Processed ${#notes[@]} packages"
echo ""

# Build batch request
notes_json=$(printf '%s\n' "${notes[@]}" | jq -s '.')
batch_request=$(jq -n --argjson notes "$notes_json" '{ notes: $notes }')

# Send batch request
echo "Importing to nixonnote..."
response=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/notes/batch" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $NOTE_TOKEN" \
    -d "$batch_request")

http_code=$(echo "$response" | tail -n1)
response_body=$(echo "$response" | sed '$d')

echo ""
if [ "$http_code" = "201" ]; then
    imported=$(echo "$response_body" | jq -r '.note_ids | length')
    failed=$(echo "$response_body" | jq -r '.failed_count')

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "${GREEN}✓ Successfully imported $imported packages${NC}"
    if [ "$failed" -gt 0 ]; then
        echo -e "${YELLOW}⚠ $failed packages failed to import${NC}"
    fi

    echo ""
    echo "Query your packages:"
    echo "  curl \"$API_URL/api/notes?q=homebrew\""
else
    echo -e "${RED}✗ Import failed (HTTP $http_code)${NC}"
    echo "$response_body" | jq '.' 2>/dev/null || echo "$response_body"
    exit 1
fi
