# Deployment Guide

This guide covers the canonical macOS deployment: immutable runtime artifacts
managed by a Homebrew service. Do not install a second standalone LaunchAgent;
two service managers can compete for port 9999.

## Quick Start

```bash
# 1. Install the formula from your tap
brew install <your-tap>/nixonnote

# 2. Configure production without putting secrets in a plist
mkdir -p ~/.config/nixonnote
$EDITOR ~/.config/nixonnote/env

# 3. Build, publish, restart, and verify
make deploy

# 4. Check status
make status
```

The service will now:
- Start automatically on login
- Restart automatically if it crashes
- Listen on port 9999
- Log under `$(brew --prefix)/var/log/nixonnote.*.log`
- Execute a published binary outside `target/`

## Service Management

The Makefile is the canonical operator interface. `service.sh` remains as a
compatibility wrapper and delegates to the same Homebrew commands.

```bash
make deploy       # Build, atomically publish, restart, and health-check
make start        # Start the Homebrew service
make stop         # Stop the Homebrew service
make restart      # Restart after configuration changes
make status       # Show Homebrew state and safe health details
make logs         # Follow Homebrew service logs
make uninstall    # Stop service; preserve formula, runtime, and data
```

`make deploy` publishes a release under
`~/Library/Application Support/NixonNote/runtime/releases/` and atomically
updates `runtime/current`. If the web root or `/api/status` fails after the
restart, it restores the previous runtime and restarts again. `make clean`
only removes checkout build products; it cannot remove the deployed runtime.

## Configuration

### Environment Variables

Production configuration lives in `~/.config/nixonnote/env` as shell-compatible
`NAME=value` assignments. The service does not read `.envrc` from the checkout,
which avoids macOS restrictions when the checkout is on an external volume.

| Variable | Default | Description |
|----------|---------|-------------|
| `APP_PORT` | `9999` | Production port; exported to the app as `NOTE_PORT` |
| `NOTE_DB` | `~/Library/Application Support/NixonNote/data/note.db` | SQLite database path |
| `NOTE_WEB_DIR` | deployed runtime web directory | Set by the service wrapper |
| `NOTE_TOKEN` | (unset) | Bearer token for API auth. Required before exposing beyond localhost. |
| `RUST_LOG` | `note=info` | Log level |

After editing the environment file, run:

```bash
make restart
```

### Adding Authentication

Set `NOTE_TOKEN` in `~/.config/nixonnote/env`:

```bash
NOTE_TOKEN=your-secret-token-here
```

Then restart:

```bash
make restart
```

All API requests will then require an `Authorization` header with your bearer token.

Do not expose NixonNote to the public internet without `NOTE_TOKEN` and a trusted network boundary such as Tailscale or a reverse proxy with authentication. The only unauthenticated API endpoint is `/api/status`.

## Logs

Logs are written to:
- `$(brew --prefix)/var/log/nixonnote.stdout.log` - Application output
- `$(brew --prefix)/var/log/nixonnote.stderr.log` - Errors and warnings

View logs:

```bash
# Last 50 lines
./service.sh logs

# Follow live
./service.sh logs -f

# Or use tail directly
tail -f "$(brew --prefix)/var/log/nixonnote."*.log
```

## Service Files

| File | Location | Purpose |
|------|----------|---------|
| Homebrew job | `~/Library/LaunchAgents/homebrew.mxcl.nixonnote.plist` | Sole app service registration |
| Runtime pointer | `~/Library/Application Support/NixonNote/runtime/current` | Active immutable release |
| Binary | `runtime/current/note` | Deployed executable |
| Web assets | `runtime/current/web` | Deployed frontend |
| Environment | `~/.config/nixonnote/env` | Production configuration and secrets |
| Database | configured `NOTE_DB` path | Persistent SQLite data |

## Troubleshooting

### Service won't start

```bash
# Check status
./service.sh status

# View error logs
./service.sh logs

# Common issues:
# - Runtime missing: make deploy
# - Port already in use: lsof -i :9999
# - Formula missing: brew install <your-tap>/nixonnote
```

### Port already in use

```bash
# Find what's using port 9999
lsof -i :9999

# Kill the process
kill <PID>

# Or change APP_PORT in ~/.config/nixonnote/env and restart
make restart
```

### Service keeps crashing

```bash
# View crash logs
./service.sh logs

# Republish the last successfully built source
make deploy
```

### Database locked

SQLite uses WAL mode with `busy_timeout = 5000ms`. If you see "database is locked" errors:

```bash
# Check for stale locks
lsof note.db*

# Restart the service
./service.sh restart
```

## Updating the Service

After making code changes:

```bash
# Build, publish, restart, and verify with rollback on failure
make deploy
```

## Remote Access (Tailscale)

To access the service from other devices:

1. Install [Tailscale](https://tailscale.com/) on your Mac
2. Connect to your Tailnet
3. Access from any device on your Tailnet:
   ```
   http://your-mac-hostname.tailscale:9999
   ```

No need to open ports or configure firewalls.

## Backup with Litestream

Litestream is configured in `litestream.yml` for continuous SQLite replication to S3.

1. Set its environment variables in a separate, private Litestream service
   configuration. Do not put them in NixonNote's Homebrew job.

2. Run litestream separately:
   ```bash
   litestream replicate -config litestream.yml
   ```

Or create a separate LaunchAgent for litestream (not covered in this guide).

## Production Deployment

For production use:

1. **Enable authentication**: Set `NOTE_TOKEN`
2. **Use HTTPS**: Put behind Caddy or nginx with TLS
3. **Set up backups**: Configure Litestream
4. **Monitor logs**: Set up log rotation
5. **Restrict network access**: Use Tailscale or firewall rules

## Uninstalling

```bash
# Stop the Homebrew service; runtime and data are preserved
./service.sh uninstall

# Uninstall the formula if desired
brew uninstall nixonnote
```

Delete the runtime, database, or logs only after backing them up and verifying
their exact configured paths. Service removal intentionally leaves them alone.
