# NixonNote

A self-hosted, AI-powered personal knowledge system. Capture thoughts (text, voice), AI auto-organizes them, search and browse your externalized memory.

## Project status

NixonNote is a personal project, published as a reference implementation rather than a product intended for broad use. It is opinionated, macOS-first, and shaped around one person's local workflow.

It is meant to run locally on your own machine or private network. Do not host it publicly. If you expose it beyond localhost, put it behind a private network such as Tailscale or a trusted reverse proxy, set `NOTE_TOKEN`, and assume the app is handling private personal notes.

## Features

- **Zero-friction capture** - CLI, web UI, and voice input
- **AI auto-organization** - Claude API tags and summarizes automatically
- **Powerful search** - Full-text search (FTS5) + vector similarity (sqlite-vec)
- **Local-first** - SQLite database, runs entirely on your machine
- **Voice transcription** - Local Whisper via `simple_transcribe_rs` for speech-to-text
- **Background processing** - Async embedding generation and auto-organization

## Setup

### Prerequisites

- **Rust** (1.70+): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** (18+) or **Bun**: For building the web frontend
- **macOS**: Currently only tested on macOS (Apple Silicon recommended)

### Optional AI Services

Configure these for enhanced features:

1. **Claude API** (auto-tagging and summarization)
   - Get your key at [console.anthropic.com](https://console.anthropic.com/)
   - Add to `com.scott.note.plist`: `<key>ANTHROPIC_API_KEY</key><string>your-anthropic-key</string>`

2. **Gemini API** (conversational chat)
   - Get your key at [aistudio.google.com/apikey](https://aistudio.google.com/apikey)
   - Add to `com.scott.note.plist`: `<key>GEMINI_API_KEY</key><string>your-gemini-key</string>`

3. **Ollama** (local embeddings for vector search)
   - Install: [ollama.ai](https://ollama.ai/)
   - Pull model: `ollama pull nomic-embed-text`
   - Will auto-connect to `http://localhost:11434`

Embeddings and transcription are local by design. Cloud APIs are optional and only used for higher-level language features such as auto-organization, chat, and TTS.

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Install as Service (Recommended)

Run as a macOS service that starts automatically and restarts on crashes:

```bash
./service.sh install
```

The service will:
- Start automatically on login
- Restart automatically if it crashes
- Listen on port 9999
- Log to `~/Library/Logs/note.*.log`

### 3. Verify

```bash
# Check status
./service.sh status

# Test connectivity
./service.sh test

# Follow logs
./service.sh logs -f
```

### 4. Access

- **Web UI**: http://localhost:9999
- **CLI**: `./target/release/note "your thought here"`

## Service Management

```bash
./service.sh install      # Build, install, and start
./service.sh start        # Start the service
./service.sh stop         # Stop the service
./service.sh restart      # Restart the service
./service.sh status       # Show status and recent logs
./service.sh logs         # Show full logs
./service.sh logs -f      # Follow logs (live)
./service.sh reload       # Reload config after editing plist
./service.sh test         # Test if service is responding
./service.sh uninstall    # Remove the service completely
```

### Homebrew Service (Alternative)

You can also run NixonNote as a Homebrew-managed service if you package it with a local or published tap. This starts automatically on login and is managed with `brew services`.

**Install:**

```bash
brew install <your-tap>/nixonnote
```

**Manage:**

```bash
brew services start nixonnote     # Start and enable at login
brew services stop nixonnote      # Stop the service
brew services restart nixonnote   # Restart after rebuilding
brew services list | grep nixonnote  # Check status
```

**Logs:** `/opt/homebrew/var/log/nixonnote.stdout.log` and `.stderr.log`

**Environment:** The brew service sources env vars from `~/.config/nixonnote/env` (not `.envrc` directly, due to macOS TCC restrictions on external volumes). After changing `.envrc`, sync it:

```bash
cp .envrc ~/.config/nixonnote/env
brew services restart nixonnote
```

**Dev workflow:** The brew service runs your local release binary, so changes take effect after a rebuild:

```bash
cargo build --release
brew services restart nixonnote
```

## Configuration

### Environment Variables

Edit your LaunchAgent plist or environment file to configure:

#### Core Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `NOTE_PORT` | `9999` | Port to listen on |
| `NOTE_DB` | `./note.db` | SQLite database path |
| `NOTE_WEB_DIR` | `./web/dist` | Frontend static files directory |
| `NOTE_TOKEN` | (unset) | Bearer token for API auth. Required before exposing beyond localhost. |
| `RUST_LOG` | `note=info` | Log level |

#### AI Services (Optional)

| Variable | Description | How to Get |
|----------|-------------|------------|
| `ANTHROPIC_API_KEY` | Claude API for auto-tagging and summarization | [Get key at console.anthropic.com](https://console.anthropic.com/) |
| `GEMINI_API_KEY` | Gemini API for conversational chat | [Get key at aistudio.google.com](https://aistudio.google.com/apikey) |
| `OLLAMA_URL` | Ollama endpoint for local embeddings | Default: `http://localhost:11434`. Uses `nomic-embed-text`; keep the same model for all indexed notes unless you re-embed everything. |

**Note**: All AI features are optional and will gracefully degrade if not configured. The app will still function for basic note capture and search.

After editing service configuration, reload:

```bash
./service.sh reload
```

### Authentication

To enable authentication, set `NOTE_TOKEN` in your environment or LaunchAgent config:

```xml
<key>NOTE_TOKEN</key>
<string>your-secret-token-here</string>
```

All API requests will then require an `Authorization` header with your bearer token.

Do not expose NixonNote to the public internet without `NOTE_TOKEN` and a trusted network boundary such as Tailscale or a reverse proxy with authentication. The only unauthenticated API endpoint is `/api/status`.

## Local AI decisions

NixonNote intentionally keeps the two high-volume, privacy-sensitive AI paths local:

- **Embeddings run locally through Ollama** using `nomic-embed-text`. Notes and search queries are embedded on your machine, then stored/searched in sqlite-vec. This keeps routine indexing cheap, avoids sending every note to an embedding API, and preserves one consistent 768-dimensional vector space. If you change embedding models, re-embed all notes; mixing models makes vector search garbage.
- **Voice transcription runs locally through Whisper** using `simple_transcribe_rs`. Browser recordings are converted with ffmpeg and transcribed on-device. Whisper models download into `WHISPER_MODEL_DIR` on first use, with `WHISPER_MODEL_SIZE=medium` by default. This avoids uploading raw voice memos to a third-party transcription service.
- **Cloud LLMs are used only where they add higher-level reasoning or generation.** Claude handles auto-tagging/summarization, Gemini handles chat, and OpenAI/Gemini/ElevenLabs can be used for TTS. Those are optional and degrade gracefully when keys are missing.

The guiding tradeoff is simple: local for personal data plumbing and repeated background work; cloud only for optional synthesis/generation features where local models were not the goal of this personal build.


## Development

### Run in Development Mode

```bash
# Terminal 1: Backend
cargo run

# Terminal 2: Frontend
cd web
bun install
bun run dev
```

Frontend dev server runs on http://localhost:9999 and proxies `/api` requests to the backend on port 8999.

### Database Migrations

Migrations are in `src/db/migrations.rs` and run automatically on startup using `rusqlite_migration`.

## macOS Integration

Import your Homebrew packages and browser bookmarks as searchable notes:

### Homebrew Packages

Import all installed packages with metadata:

```bash
./scripts/import-homebrew.sh
```

This creates one note per package with version, description, and homepage. Each note is:
- Tagged with `hidden` (to filter from default view) and `tool`
- Tagged with `source_type: "homebrew"` and `source_url` set to the package name for deduplication

**Example output:**
```
🍺 Homebrew Package Import
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Found 183 Homebrew packages

Fetching package metadata...
...........................................
Processed 183 packages

Importing to nixonnote...

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ Successfully imported 183 packages

Query your packages:
  curl "http://localhost:9999/api/notes?q=homebrew"
```

### Browser Bookmarks

Import Microsoft Edge or Brave bookmarks (both use the same format):

```bash
./scripts/import-edge-bookmarks.sh
```

This recursively walks your bookmark folders and creates one note per bookmark with folder context. Each note is:
- Tagged with `hidden` (to filter from default view) and `bookmark`
- Tagged with `source_type: "bookmark"` and `source_url` set to the URL for deduplication

**Custom bookmark file location:**
```bash
EDGE_BOOKMARKS="$HOME/Library/Application Support/BraveSoftware/Brave-Browser/Default/Bookmarks" \
  ./scripts/import-edge-bookmarks.sh
```

**Example note structure:**
```markdown
# GitHub - anthropics/claude-code

**Folder:** Work / Development

**URL:** https://github.com/anthropics/claude-code
```

### Configuration

Both scripts respect these environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `NOTE_API_URL` | `http://localhost:9999` | API endpoint |
| `NOTE_TOKEN` | (empty) | Bearer token for authentication |

### Querying Imported Data

After importing, search using FTS5 or vector similarity:

```bash
# Find all Homebrew packages
curl "http://localhost:9999/api/notes?q=homebrew"

# Find a specific package
curl "http://localhost:9999/api/notes?q=rust compiler"

# Find bookmarks
curl "http://localhost:9999/api/notes?q=bookmark github"
```

Or use the web UI at http://localhost:9999 to browse and search.

### Filtering Hidden Items

Imported items are tagged with `hidden` to keep them out of your default note stream. To include them in searches, explicitly filter by tag:

```bash
# Show all hidden items
curl "http://localhost:9999/api/tags/filter?tag=hidden"

# Show all tools (Homebrew packages)
curl "http://localhost:9999/api/tags/filter?tag=tool"

# Show all bookmarks
curl "http://localhost:9999/api/tags/filter?tag=bookmark"
```

The web UI can be updated to exclude notes with the `hidden` tag from the default view.

## CLI Usage

```bash
# Add a note
./target/release/note "This is my thought"

# Add to specific note
./target/release/note "Additional context" --parent-id 123
```

Notes are saved immediately to SQLite. Background tasks will auto-organize them asynchronously.

## API Endpoints

### Notes
- `GET /api/notes` - List notes (with pagination, search, tags)
- `POST /api/notes` - Create single note
- `POST /api/notes/batch` - Create multiple notes in one transaction (up to 1000)
- `GET /api/notes/{id}` - Get note by ID
- `PUT /api/notes/{id}` - Update note content
- `DELETE /api/notes/{id}` - Delete note

### Tags & Organization
- `GET /api/tags` - List all tags with counts
- `GET /api/tags/filter` - Filter notes by tag

### AI Features
- `POST /api/voice` - Transcribe voice recording
- `POST /api/chat` - Chat with AI about your notes
- `POST /api/chat/stream` - Streaming chat response

### Batch Import Example

```bash
curl -X POST http://localhost:9999/api/notes/batch \
  -H "Content-Type: application/json" \
  -d '{
    "notes": [
      {
        "content": "# My First Note\n\nContent here",
        "source_type": "import",
        "source_url": "optional-dedup-key",
        "tags": ["hidden", "archived"]
      },
      {
        "content": "# My Second Note\n\nMore content",
        "source_type": "import",
        "tags": ["draft"]
      }
    ]
  }'
```

**Response:**
```json
{
  "note_ids": [123, 124],
  "failed_count": 0
}
```

## Tech Stack

| Component | Technology | Why |
|-----------|-----------|-----|
| Backend | Rust + Axum 0.8 | Performance, single binary, excellent SQLite support |
| Database | SQLite (rusqlite) | Single file, zero ops, full SQL, FTS5 built-in |
| Vector search | sqlite-vec | Same DB as metadata, no additional infrastructure |
| Async pool | deadpool-sqlite | Bridge between sync rusqlite and async Axum |
| Migrations | rusqlite_migration | Lightweight, uses `user_version` pragma |
| Local embeddings | Ollama (nomic-embed-text) | 768-dim embeddings, runs on Apple Silicon |
| Local transcription | `simple_transcribe_rs` + Whisper | On-device transcription; ffmpeg converts browser audio to 16 kHz mono WAV first |
| Auto-org LLM | Claude API (Sonnet) | Structured output via `tool_use` |
| Frontend | React + Vite + Tailwind | Minimal stack, fast HMR |

## RAG Architecture

NixonNote uses **Naive RAG** (Retrieval-Augmented Generation) to answer questions about your notes via the chat interface. This is the simplest RAG pattern: a linear pipeline of index → retrieve → generate with no query rewriting, reranking, or agentic reasoning loops.

### Pipeline

```
User Question
    │
    ▼
┌─────────────────────┐
│  1. Embed Query      │  Ollama (nomic-embed-text) generates a 768-dim vector
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  2. Retrieve Notes   │  sqlite-vec cosine similarity search (fallback: FTS5)
└────────┬────────────┘
         │  Top-K notes (default 5)
         ▼
┌─────────────────────┐
│  3. Build Context    │  Concatenate title + summary + content of matched notes
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  4. Generate Answer  │  LLM synthesizes answer citing note IDs
└─────────────────────┘
```

### Components

| Stage | Technology | Details |
|-------|-----------|---------|
| Embedding (indexing) | Ollama + nomic-embed-text | 768-dim vectors, stored in sqlite-vec. Runs async on note creation via background job |
| Embedding (query) | Ollama + nomic-embed-text | Same model embeds the user's question at query time |
| Vector store | sqlite-vec | SQLite extension, cosine similarity search, no external infrastructure |
| Full-text fallback | SQLite FTS5 | Used when Ollama is unavailable or vector search returns no results |
| Generation | Gemini 2.5 Flash (default) or Claude Sonnet | User-selectable per request via `llm` parameter |
| Streaming | Gemini Interactions API | SSE streaming for real-time chat responses |

### How Embeddings Are Generated

When a note is created, a background job:
1. Combines the note's title, content, and AI-generated summary
2. Truncates to 8,000 characters (model's context limit)
3. Sends to Ollama's `/api/embed` endpoint (`OLLAMA_URL`, default `http://localhost:11434`)
4. Stores the resulting 768-dim float vector in the `note_embeddings` table via sqlite-vec

### Search Flow

The chat endpoint (`POST /api/chat`) performs hybrid search:
1. **Vector search** (primary): Embeds the query, runs cosine similarity against all note embeddings
2. **FTS5 search** (fallback): If embedding generation fails (e.g., Ollama offline), falls back to SQLite full-text search
3. Retrieved notes are passed as context to the LLM with the user's question

## Remote Access

Use [Tailscale](https://tailscale.com/) for secure remote access without port forwarding:

1. Install Tailscale on your Mac
2. Connect to your Tailnet
3. Access from any device: `http://your-mac-hostname.tailscale:9999`

## Backup

Litestream is configured for continuous SQLite replication to S3. See `litestream.yml` and [DEPLOYMENT.md](DEPLOYMENT.md) for setup instructions.

## Logs

Logs are written to:
- `~/Library/Logs/note.stdout.log` - Application output
- `~/Library/Logs/note.stderr.log` - Errors and warnings

```bash
# View logs
./service.sh logs

# Follow logs live
./service.sh logs -f

# Or use tail directly
tail -f ~/Library/Logs/note.*.log
```

## Troubleshooting

### Service won't start

```bash
# Check logs
./service.sh logs

# Common issues:
# - Port already in use: lsof -i :9999
# - Binary not built: cargo build --release
```

### Port already in use

```bash
# Find process using port 9999
lsof -i :9999

# Kill it
kill <PID>
```

### Database locked

SQLite uses WAL mode with `busy_timeout = 5000ms`. If you see "database is locked":

```bash
./service.sh restart
```

### AI Features Not Working

**Chat shows "GEMINI_API_KEY not set" error**:
1. Add your Gemini API key to `com.scott.note.plist`
2. Reload: `./service.sh reload`
3. Get a key at: https://aistudio.google.com/apikey

**Notes not being auto-tagged**:
1. Check if `ANTHROPIC_API_KEY` is set in `com.scott.note.plist`
2. Check logs for errors: `./service.sh logs | grep -i anthropic`
3. Get a key at: https://console.anthropic.com/

**Vector search not working (falling back to FTS)**:
1. Install Ollama: https://ollama.ai/
2. Pull the model: `ollama pull nomic-embed-text`
3. Verify Ollama is running: `curl http://localhost:11434/api/version`
4. Check logs: `./service.sh logs | grep -i ollama`

## Documentation

- [DEPLOYMENT.md](DEPLOYMENT.md) - Complete deployment guide
- [SECURITY.md](SECURITY.md) - Local-first security guidance

## Project Structure

```
.
├── src/
│   ├── main.rs              # Entry point, server setup
│   ├── db/
│   │   ├── mod.rs          # Database setup, connection pool
│   │   ├── migrations.rs   # Schema migrations
│   │   └── queries.rs      # SQL queries
│   ├── routes/
│   │   ├── notes.rs        # Notes CRUD endpoints
│   │   ├── tags.rs         # Tag endpoints
│   │   ├── voice.rs        # Voice transcription
│   │   └── chat.rs         # AI chat endpoint
│   └── background/
│       ├── mod.rs          # Background task processor
│       ├── embed.rs        # Embedding generation (Ollama)
│       └── auto_org.rs     # Auto-organization (Claude)
├── web/                     # React frontend
├── com.scott.note.plist    # macOS service configuration
├── service.sh              # Service management script
└── litestream.yml          # SQLite backup configuration
```

## License

MIT
