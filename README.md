# NixonNote

A self-hosted, AI-powered personal knowledge system. Capture thoughts (text, voice), AI auto-organizes them, search and browse your externalized memory.

## Project status

NixonNote is a personal project, published as a reference implementation rather than a product intended for broad use. It is opinionated, macOS-first, and shaped around one person's local workflow.

It is meant to run locally on your own machine. By default the server binds to `127.0.0.1` only, so it is not reachable from your LAN or from other devices. Do not host it publicly, and do not set `NOTE_HOST` to a non-loopback address without also setting `NOTE_TOKEN`. For remote access from your other devices, prefer `tailscale serve` (see [Remote Access](#remote-access)) over widening the bind address.

## Features

- **Zero-friction capture** - CLI, web UI, and voice input
- **AI auto-organization** - Claude API tags and summarizes automatically
- **Powerful search** - Full-text search (FTS5) + vector similarity (sqlite-vec)
- **Local-first** - SQLite database, runs entirely on your machine
- **Voice transcription** - Osaurus (Whisper) for local speech-to-text
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
   - Add `ANTHROPIC_API_KEY=...` to `~/.config/nixonnote/env`

2. **Gemini API** (conversational chat)
   - Get your key at [aistudio.google.com/apikey](https://aistudio.google.com/apikey)
   - Add `GEMINI_API_KEY=...` to `~/.config/nixonnote/env`

3. **Ollama** (local embeddings for vector search)
   - Install: [ollama.ai](https://ollama.ai/)
   - Pull model: `ollama pull nomic-embed-text`
   - Will auto-connect to `http://localhost:11434`

All AI features are optional and the app will function without them.

## Quick Start

### 1. Install the Homebrew service

```bash
brew install <your-tap>/nixonnote
```

### 2. Configure

Put production environment variables in `~/.config/nixonnote/env`. This local
file is used instead of reading `.envrc` from the checkout, which avoids macOS
LaunchAgent restrictions on external volumes.

### 3. Build and deploy

```bash
make deploy
```

Deployment builds the backend and frontend, publishes both under
`~/Library/Application Support/NixonNote/runtime`, restarts Homebrew, and
checks the web app and API. The published runtime is independent of `target/`
and `web/dist`, so `make clean` cannot break the running service.

The Homebrew service will:
- Start automatically on login
- Restart automatically if it crashes
- Listen on port 9999
- Log under `$(brew --prefix)/var/log/nixonnote.*.log`

### 4. Verify

```bash
# Check status
./service.sh status

# Test connectivity
./service.sh test

# Follow logs
./service.sh logs -f
```

### 5. Access

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
./service.sh reload       # Restart after editing ~/.config/nixonnote/env
./service.sh test         # Test if service is responding
./service.sh uninstall    # Stop the Homebrew service; preserve runtime and data
```

### Homebrew Service

Homebrew is the sole production service manager. The old standalone
`com.nixonnote.app`/`com.scott.note` LaunchAgent path has been retired because
running both service managers can create a port conflict.

**Manage:**

```bash
brew services start nixonnote     # Start and enable at login
brew services stop nixonnote      # Stop the service
brew services restart nixonnote   # Restart after rebuilding
brew services list | grep nixonnote  # Check status
```

**Logs:** `$(brew --prefix)/var/log/nixonnote.stdout.log` and `.stderr.log`

**Environment:** The brew service sources env vars from `~/.config/nixonnote/env` (not `.envrc` directly, due to macOS TCC restrictions on external volumes). After changing `.envrc`, sync it:

```bash
cp .envrc ~/.config/nixonnote/env
brew services restart nixonnote
```

**Deployment workflow:**

```bash
make deploy
```

Each deployment creates an immutable release and atomically updates the
`runtime/current` symlink. If either the web root or `/api/status` fails after
restart, deployment restores the previous runtime and restarts the service.

## Configuration

### Environment Variables

Edit `~/.config/nixonnote/env` to configure production:

#### Core Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `APP_PORT` | `9999` | Production port; exported to the app as `NOTE_PORT` |
| `NOTE_HOST` | `127.0.0.1` | Bind address. Parsed as an IP address; an invalid value fails startup instead of falling back to `0.0.0.0`. |
| `NOTE_DB` | `~/Library/Application Support/NixonNote/data/note.db` | SQLite database path |
| `NOTE_WEB_DIR` | deployed runtime web directory | Set by the service wrapper |
| `NOTE_TOKEN` | (unset) | Bearer token for API auth. Required before exposing beyond localhost. |
| `RUST_LOG` | `note=info` | Log level |

#### AI Services (Optional)

| Variable | Description | How to Get |
|----------|-------------|------------|
| `ANTHROPIC_API_KEY` | Claude API for auto-tagging and summarization | [Get key at console.anthropic.com](https://console.anthropic.com/) |
| `GEMINI_API_KEY` | Gemini API for conversational chat | [Get key at aistudio.google.com](https://aistudio.google.com/apikey) |
| `OLLAMA_URL` | Ollama endpoint for local embeddings | Default: `http://localhost:11434`. [Install Ollama](https://ollama.ai/), then run `ollama pull nomic-embed-text` |

**Note**: All AI features are optional and will gracefully degrade if not configured. The app will still function for basic note capture and search.

After editing service configuration, restart:

```bash
make restart
```

### Authentication

To enable authentication, set `NOTE_TOKEN` in `~/.config/nixonnote/env`:

```bash
NOTE_TOKEN=your-secret-token-here
```

All API requests will then require an `Authorization` header with your bearer token.

Do not expose NixonNote to the public internet without `NOTE_TOKEN` and a trusted network boundary such as Tailscale or a reverse proxy with authentication. The only unauthenticated API endpoint is `/api/status`.

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
| Local transcription | Osaurus (Whisper) | Apple Silicon optimized, OpenAI-compatible API |
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
3. Sends to Ollama's `/api/embed` endpoint
4. Stores the resulting 768-dim float vector in the `note_embeddings` table via sqlite-vec

### Search Flow

The chat endpoint (`POST /api/chat`) performs hybrid search:
1. **Vector search** (primary): Embeds the query, runs cosine similarity against all note embeddings
2. **FTS5 search** (fallback): If embedding generation fails (e.g., Ollama offline), falls back to SQLite full-text search
3. Retrieved notes are passed as context to the LLM with the user's question

## Remote Access

NixonNote binds to `127.0.0.1` by default, so it is not reachable from other
devices until you choose one of the options below.

### Recommended: `tailscale serve`

[Tailscale](https://tailscale.com/) can proxy a request from your tailnet to
the loopback-only server, so you get remote access without widening the bind
address or opening a port:

1. Install Tailscale on your Mac and connect to your Tailnet.
2. Run:
   ```bash
   /Applications/Tailscale.app/Contents/MacOS/Tailscale serve --bg 9999
   ```
   (Use the app bundle's `Tailscale` binary, not a separately installed CLI;
   a mismatched CLI can fail with a "bundleIdentifier is unknown" error.)
3. Access from any device on your Tailnet: `https://your-mac-hostname.your-tailnet.ts.net`
4. Check status any time with `tailscale serve status`, and remove it with `tailscale serve clear`.

### Alternative: widen the bind address

If you need the raw `host:9999` address to be reachable (for example a local
phone client that cannot use the tailnet hostname), set `NOTE_HOST` in
`~/.config/nixonnote/env`:

```bash
NOTE_HOST=0.0.0.0
```

or bind only to your Tailscale interface address instead of all interfaces.
Whenever `NOTE_HOST` is not a loopback address, also set `NOTE_TOKEN` — the
API has no other authentication, and anyone who can reach the bound address
can read and write your notes.

## Backup

Litestream is configured for continuous SQLite replication to S3. See `litestream.yml` and [DEPLOYMENT.md](DEPLOYMENT.md) for setup instructions.

## Logs

Logs are written to:
- `$(brew --prefix)/var/log/nixonnote.stdout.log` - Application output
- `$(brew --prefix)/var/log/nixonnote.stderr.log` - Errors and warnings

```bash
# View logs
./service.sh logs

# Follow logs live
./service.sh logs -f

# Or use tail directly
tail -f "$(brew --prefix)/var/log/nixonnote."*.log
```

## Troubleshooting

### Service won't start

```bash
# Check logs
./service.sh logs

# Common issues:
# - Port already in use: lsof -i :9999
# - Runtime missing: make deploy
# - Formula missing: brew install <your-tap>/nixonnote
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
1. Add your Gemini API key to `~/.config/nixonnote/env`
2. Restart: `make restart`
3. Get a key at: https://aistudio.google.com/apikey

**Notes not being auto-tagged**:
1. Check if `ANTHROPIC_API_KEY` is set in `~/.config/nixonnote/env`
2. Check logs for errors: `./service.sh logs | grep -i anthropic`
3. Get a key at: https://console.anthropic.com/

**Vector search not working (falling back to FTS)**:
1. Install Ollama: https://ollama.ai/
2. Pull the model: `ollama pull nomic-embed-text`
3. Verify Ollama is running: `curl http://localhost:11434/api/version`
4. Check logs: `./service.sh logs | grep -i ollama`

## Documentation

- [DEPLOYMENT.md](DEPLOYMENT.md) - Complete deployment guide
- [AGENTS.md](AGENTS.md) - Agent workflows and automation
- [docs/plans/](docs/plans/) - Implementation plans and milestones

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
├── bin/deploy-service       # Immutable Homebrew runtime publisher
├── service.sh               # Compatibility wrapper for Make/Homebrew
└── litestream.yml          # SQLite backup configuration
```

## License

MIT
