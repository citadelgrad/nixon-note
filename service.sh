#!/usr/bin/env bash
#
# Manage the note service (macOS LaunchAgent)
#

set -euo pipefail

PLIST_NAME="com.scott.note.plist"
PLIST_SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$PLIST_NAME"
PLIST_DEST="$HOME/Library/LaunchAgents/$PLIST_NAME"
SERVICE_LABEL="com.scott.note"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

cmd_install() {
    # Build frontend
    log_info "Building frontend..."
    (cd web && bun install && bun run build)

    # Build release binary
    log_info "Building release binary..."
    cargo build --release

    # Copy plist to LaunchAgents
    log_info "Installing service configuration..."
    mkdir -p "$HOME/Library/LaunchAgents"
    cp "$PLIST_SRC" "$PLIST_DEST"

    # Load the service
    log_info "Loading service..."
    launchctl load "$PLIST_DEST" 2>/dev/null || true
    launchctl start "$SERVICE_LABEL" 2>/dev/null || true

    log_info "Service installed and started!"
    log_info "Logs: ~/Library/Logs/note.*.log"
    log_info "Status: ./service.sh status"
}

cmd_uninstall() {
    log_info "Stopping service..."
    launchctl stop "$SERVICE_LABEL" 2>/dev/null || true
    launchctl unload "$PLIST_DEST" 2>/dev/null || true

    log_info "Removing service configuration..."
    rm -f "$PLIST_DEST"

    log_info "Service uninstalled!"
}

cmd_start() {
    log_info "Starting service..."
    launchctl start "$SERVICE_LABEL"
    log_info "Service started!"
}

cmd_stop() {
    log_info "Stopping service..."
    launchctl stop "$SERVICE_LABEL"
    log_info "Service stopped!"
}

cmd_restart() {
    log_info "Restarting service..."
    launchctl stop "$SERVICE_LABEL" 2>/dev/null || true
    sleep 1
    launchctl start "$SERVICE_LABEL"
    log_info "Service restarted!"
}

cmd_status() {
    echo "Service status for: $SERVICE_LABEL"
    echo ""

    if launchctl list | grep -q "$SERVICE_LABEL"; then
        log_info "Service is loaded"
        launchctl list "$SERVICE_LABEL"
    else
        log_warn "Service is NOT loaded"
    fi

    echo ""
    echo "Recent logs (stdout):"
    tail -20 "$HOME/Library/Logs/note.stdout.log" 2>/dev/null || echo "(no logs yet)"

    echo ""
    echo "Recent logs (stderr):"
    tail -20 "$HOME/Library/Logs/note.stderr.log" 2>/dev/null || echo "(no logs yet)"
}

cmd_logs() {
    local follow="${1:-}"

    if [[ "$follow" == "-f" || "$follow" == "--follow" ]]; then
        log_info "Following logs (Ctrl+C to stop)..."
        tail -f "$HOME/Library/Logs/note.stdout.log" "$HOME/Library/Logs/note.stderr.log"
    else
        echo "=== STDOUT ==="
        tail -50 "$HOME/Library/Logs/note.stdout.log" 2>/dev/null || echo "(no logs yet)"
        echo ""
        echo "=== STDERR ==="
        tail -50 "$HOME/Library/Logs/note.stderr.log" 2>/dev/null || echo "(no logs yet)"
    fi
}

cmd_reload() {
    log_info "Reloading service configuration..."
    launchctl unload "$PLIST_DEST" 2>/dev/null || true
    cp "$PLIST_SRC" "$PLIST_DEST"
    launchctl load "$PLIST_DEST"
    log_info "Configuration reloaded!"
}

cmd_test() {
    log_info "Testing service connectivity..."

    if curl -s -o /dev/null -w "%{http_code}" http://localhost:9999/api/notes | grep -q "200\|401"; then
        log_info "Service is responding on port 9999!"
    else
        log_error "Service is NOT responding on port 9999"
        exit 1
    fi
}

cmd_build() {
    log_info "Building frontend..."
    (cd web && bun install && bun run build)

    log_info "Building backend..."
    cargo build --release

    log_info "Build complete!"
}

cmd_help() {
    cat << EOF
Usage: ./service.sh <command>

Commands:
    install     Build and install the service (starts automatically)
    uninstall   Stop and remove the service
    start       Start the service
    stop        Stop the service
    restart     Restart the service
    status      Show service status and recent logs
    logs [-f]   Show logs (-f to follow)
    reload      Reload service configuration after editing plist
    build       Build frontend and backend (without installing)
    test        Test if service is responding
    help        Show this help message

Examples:
    ./service.sh install        # First time setup
    ./service.sh status         # Check if running
    ./service.sh logs -f        # Watch logs live
    ./service.sh restart        # After code changes
    ./service.sh uninstall      # Remove service

Service files:
    Config:  $PLIST_DEST
    Logs:    ~/Library/Logs/note.*.log
    DB:      $(pwd)/note.db

After installation, the service will:
    - Start automatically on login
    - Restart automatically if it crashes
    - Listen on port 9999
EOF
}

# Main
COMMAND="${1:-help}"
shift || true

case "$COMMAND" in
    install)    cmd_install ;;
    uninstall)  cmd_uninstall ;;
    start)      cmd_start ;;
    stop)       cmd_stop ;;
    restart)    cmd_restart ;;
    status)     cmd_status ;;
    logs)       cmd_logs "$@" ;;
    reload)     cmd_reload ;;
    build)      cmd_build ;;
    test)       cmd_test ;;
    help)       cmd_help ;;
    *)
        log_error "Unknown command: $COMMAND"
        cmd_help
        exit 1
        ;;
esac
