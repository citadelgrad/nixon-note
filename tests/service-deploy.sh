#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/nixonnote-deploy-test.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

PROJECT="$TMP/project"
RUNTIME="$TMP/runtime"
FAKE_BIN="$TMP/bin"
CALLS="$TMP/brew-calls"
mkdir -p "$PROJECT/target/release" "$PROJECT/web/dist" "$FAKE_BIN"

cat > "$PROJECT/target/release/note" <<'EOF'
#!/usr/bin/env bash
printf 'NOTE_WEB_DIR=%s\nNOTE_DB=%s\n' "$NOTE_WEB_DIR" "$NOTE_DB"
EOF
chmod 0755 "$PROJECT/target/release/note"
printf '<div id="root"></div>\n' > "$PROJECT/web/dist/index.html"

cat > "$FAKE_BIN/brew" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$BREW_CALLS"
case "$*" in
  "services info nixonnote"|"services restart nixonnote"|"services stop nixonnote") exit 0 ;;
  *) exit 1 ;;
esac
EOF
chmod 0755 "$FAKE_BIN/brew"

cat > "$FAKE_BIN/curl" <<'EOF'
#!/usr/bin/env bash
if [ "${FAIL_HEALTH:-0}" = 1 ]; then
  exit 22
fi
exit 0
EOF
chmod 0755 "$FAKE_BIN/curl"

cat > "$FAKE_BIN/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod 0755 "$FAKE_BIN/sleep"

export PATH="$FAKE_BIN:$PATH"
export BREW_CALLS="$CALLS"
export HOME="$TMP/home"
mkdir -p "$HOME"

NIXONNOTE_PROJECT_DIR="$PROJECT" \
NIXONNOTE_RUNTIME_DIR="$RUNTIME" \
  "$ROOT/bin/deploy-service" > "$TMP/deploy.out"

[ -L "$RUNTIME/current" ]
[ -x "$RUNTIME/current/note" ]
[ -f "$RUNTIME/current/web/index.html" ]
grep -q '^services restart nixonnote$' "$CALLS"
previous_target="$(readlink "$RUNTIME/current")"

rm -rf "$PROJECT/target" "$PROJECT/web/dist"
[ -x "$RUNTIME/current/note" ]
[ -f "$RUNTIME/current/web/index.html" ]

mkdir -p "$PROJECT/target/release" "$PROJECT/web/dist"
cp "$RUNTIME/current/note" "$PROJECT/target/release/note"
printf '<div id="root">second release</div>\n' > "$PROJECT/web/dist/index.html"

NIXONNOTE_PROJECT_DIR="$PROJECT" \
NIXONNOTE_RUNTIME_DIR="$RUNTIME" \
  "$ROOT/bin/deploy-service" > "$TMP/second-deploy.out"
second_target="$(readlink "$RUNTIME/current")"
[ "$second_target" != "$previous_target" ]
grep -Fq 'second release' "$RUNTIME/current/web/index.html"

printf '<div id="root">broken candidate</div>\n' > "$PROJECT/web/dist/index.html"

if FAIL_HEALTH=1 NIXONNOTE_PROJECT_DIR="$PROJECT" NIXONNOTE_RUNTIME_DIR="$RUNTIME" \
  "$ROOT/bin/deploy-service" > "$TMP/rollback.out" 2>&1; then
  echo "expected failed health check to roll back" >&2
  exit 1
fi
[ "$(readlink "$RUNTIME/current")" = "$second_target" ]

SERVICE_OUTPUT="$(env -u NOTE_DB -u NOTE_WEB_DIR -u APP_PORT \
  NIXONNOTE_RUNTIME_DIR="$RUNTIME" \
  NIXONNOTE_APP_SUPPORT_DIR="$TMP/app-support" \
  NIXONNOTE_ENV_FILE="$TMP/missing-env" \
  "$ROOT/bin/note-service")"
printf '%s\n' "$SERVICE_OUTPUT" | grep -Fq "NOTE_WEB_DIR=$RUNTIME/current/web"
printf '%s\n' "$SERVICE_OUTPUT" | grep -Fq "NOTE_DB=$TMP/app-support/data/note.db"

printf 'service deployment regression test passed\n'
