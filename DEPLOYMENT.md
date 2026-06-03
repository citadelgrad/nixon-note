# Deployment Guide

This guide covers running the note service as a macOS LaunchAgent.

## Quick Start

```bash
# 1. Install and start the service
./service.sh install

# 2. Check that it's running
./service.sh status

# 3. Test connectivity
./service.sh test
```

The service will now:
- Start automatically on login
- Restart automatically if it crashes
- Listen on port 9999
- Log to `~/Library/Logs/note.*.log`

## Service Management

The `service.sh` script provides all service management commands:

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

## Configuration

### Environment Variables

The service configuration is in your LaunchAgent plist or environment file. The tracked plist is a public-safe template; replace placeholders before installing.

| Variable | Default | Description |
|----------|---------|-------------|
| `NOTE_PORT` | `9999` | Port to listen on |
| `NOTE_DB` | `./note.db` | SQLite database path |
| `NOTE_WEB_DIR` | `./web/dist` | Frontend static files directory |
| `NOTE_TOKEN` | (unset) | Bearer token for API auth. Required before exposing beyond localhost. |
| `RUST_LOG` | `note=info` | Log level |

After editing the plist file, run:

```bash
./service.sh reload
```

### Adding Authentication

Set `NOTE_TOKEN` in your environment or LaunchAgent config:

```xml
<key>NOTE_TOKEN</key>
<string>your-secret-token-here</string>
```

Then reload:

```bash
./service.sh reload
```

All API requests will then require an `Authorization` header with your bearer token.

Do not expose NixonNote to the public internet without `NOTE_TOKEN` and a trusted network boundary such as Tailscale or a reverse proxy with authentication. The only unauthenticated API endpoint is `/api/status`.

## Logs

Logs are written to:
- `~/Library/Logs/note.stdout.log` - Application output
- `~/Library/Logs/note.stderr.log` - Errors and warnings

View logs:

```bash
# Last 50 lines
./service.sh logs

# Follow live
./service.sh logs -f

# Or use tail directly
tail -f ~/Library/Logs/note.*.log
```

## Service Files

| File | Location | Purpose |
|------|----------|---------|
| Plist source | `com.scott.note.plist` | Public-safe service configuration template |
| Plist installed | `~/Library/LaunchAgents/com.nixonnote.app.plist` | Active service configuration |
| Binary | `target/release/note` | Compiled application |
| Database | `note.db` | SQLite database (auto-created) |

## Troubleshooting

### Service won't start

```bash
# Check status
./service.sh status

# View error logs
./service.sh logs

# Common issues:
# - Binary not built: cargo build --release
# - Port already in use: lsof -i :9999
# - Permission issues: check file ownership
```

### Port already in use

```bash
# Find what's using port 9999
lsof -i :9999

# Kill the process
kill <PID>

# Or change the port in com.scott.note.plist and reload
./service.sh reload
```

### Service keeps crashing

```bash
# View crash logs
./service.sh logs

# Check system logs
log show --predicate 'process == "note"' --last 1h

# Uninstall and reinstall
./service.sh uninstall
./service.sh install
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
# Rebuild and restart
cargo build --release
./service.sh restart

# Or reinstall completely
./service.sh uninstall
./service.sh install
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

1. Set environment variables in `com.scott.note.plist`:
   ```xml
   <key>LITESTREAM_BUCKET</key>
   <string>your-bucket-name</string>
   <key>LITESTREAM_ENDPOINT</key>
   <string>https://s3.us-west-2.amazonaws.com</string>
   <key>LITESTREAM_ACCESS_KEY_ID</key>
   <string>your-access-key</string>
   <key>LITESTREAM_SECRET_ACCESS_KEY</key>
   <string>your-secret-key</string>
   ```

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
# Stop and remove service
./service.sh uninstall

# Optionally remove data
rm note.db note.db-shm note.db-wal

# Remove logs
rm ~/Library/Logs/note.*.log
```
