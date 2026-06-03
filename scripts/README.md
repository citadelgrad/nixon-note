# Import/Export Scripts

Scripts to migrate notes from Apple Notes (or other sources) into your note app.

## Quick Start

### Export from Apple Notes

```bash
# Export all Apple Notes to a directory
./scripts/export-apple-notes.sh

# Or specify a custom directory
./scripts/export-apple-notes.sh my-notes-backup
```

This will:
- Open Apple Notes via AppleScript
- Export each note as a text file
- Include metadata (title, created date, modified date)
- Save to `apple-notes-export/` (or your specified directory)

### Import into Note App

```bash
# Import all exported notes
./scripts/import-notes.sh apple-notes-export
```

This will:
- Read all `.txt` and `.md` files from the directory
- Strip HTML tags (Apple Notes exports can have HTML)
- Create each note via the API
- Show progress and success/failure counts

## Options

### Authentication

If you have `NOTE_TOKEN` set:

```bash
export NOTE_TOKEN="your-token-here"
./scripts/import-notes.sh apple-notes-export
```

### Custom API URL

```bash
export NOTE_API_URL="http://localhost:9999"
./scripts/import-notes.sh apple-notes-export
```

## Manual Export from Apple Notes

If the script doesn't work, you can manually export:

1. Open **Apple Notes**
2. Select a note (or multiple notes with Cmd+Click)
3. **File → Export as PDF...** or just drag notes to a folder
4. Save as `.txt` files
5. Run: `./scripts/import-notes.sh your-export-folder`

## Import from Other Sources

The import script works with any directory of text files:

```bash
# Import markdown files
./scripts/import-notes.sh ~/Documents/my-notes

# Import from a backup
./scripts/import-notes.sh ~/Backups/notes-2024
```

## Bulk Import Format

Each file should contain plain text or markdown. The entire file content becomes the note content.

Example file structure:
```
my-notes/
  ├── meeting-notes-2024.txt
  ├── ideas.md
  └── todos.txt
```

## Troubleshooting

**Error: "AppleScript execution failed"**
- Make sure Apple Notes is installed
- Grant Terminal/iTerm permission to control Notes (System Settings → Privacy & Security → Automation)

**Error: "Connection refused"**
- Make sure the note service is running: `./service.sh status`
- Check the API URL: `curl http://localhost:9999/api/notes`

**Error: "401 Unauthorized"**
- Set NOTE_TOKEN if authentication is enabled
- Check your token in `com.scott.note.plist`

**HTML tags in imported notes**
- The import script automatically strips HTML
- If you still see tags, the content might be in a different format
- Manually clean files before importing

## Features

- ✅ Preserves note content
- ✅ Strips HTML formatting
- ✅ Progress indicators
- ✅ Success/failure tracking
- ✅ Handles special characters
- ✅ Supports `.txt` and `.md` files
- ✅ Background AI processing (embeddings, auto-tagging)

After import, notes will be processed in the background to:
- Generate embeddings (for semantic search)
- Auto-tag (if Claude API is configured)
- Build full-text search index
