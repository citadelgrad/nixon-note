#!/usr/bin/env bash
# Compatibility wrapper. Homebrew is the sole production service manager.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMMAND="${1:-help}"
shift || true

case "$COMMAND" in
  install) exec make -C "$ROOT" install ;;
  uninstall) exec make -C "$ROOT" uninstall ;;
  start|stop|restart|status) exec make -C "$ROOT" "$COMMAND" ;;
  reload) exec make -C "$ROOT" restart ;;
  build) exec make -C "$ROOT" build ;;
  test)
    ENV_FILE="$HOME/.config/nixonnote/env"
    if [ -f "$ENV_FILE" ]; then
      set -a
      # shellcheck source=/dev/null
      source "$ENV_FILE"
      set +a
    fi
    PORT="${APP_PORT:-9999}"
    curl --max-time 5 -fsS "http://127.0.0.1:$PORT/" >/dev/null
    curl --max-time 5 -fsS "http://127.0.0.1:$PORT/api/status" >/dev/null
    echo "NixonNote web app and API are responding on port $PORT."
    ;;
  logs)
    if [[ "${1:-}" == "-f" || "${1:-}" == "--follow" ]]; then
      exec make -C "$ROOT" logs
    fi
    LOG_DIR="$(brew --prefix)/var/log"
    tail -50 "$LOG_DIR/nixonnote.stdout.log" "$LOG_DIR/nixonnote.stderr.log"
    ;;
  help)
    cat <<'EOF'
Usage: ./service.sh <command>

Compatibility wrapper around the canonical Makefile/Homebrew workflow:
  install     Build, publish, start, and health-check NixonNote
  uninstall   Stop the Homebrew service (preserves formula, runtime, and data)
  start       Start the Homebrew service
  stop        Stop the Homebrew service
  restart     Restart the Homebrew service
  status      Show Homebrew state and application health
  logs [-f]   Show or follow Homebrew service logs
  reload      Restart after changing ~/.config/nixonnote/env
  build       Build without deploying
  test        Smoke-test the web app and API
EOF
    ;;
  *)
    echo "Unknown command: $COMMAND" >&2
    exit 1
    ;;
esac
