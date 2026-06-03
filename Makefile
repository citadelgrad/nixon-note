# nixonnote development Makefile
# Usage: make help

SHELL := /bin/bash
.DEFAULT_GOAL := help

# Config — ports come from .envrc (APP_PORT=web app, API_PORT=backend)
# In dev: Vite serves on APP_PORT, proxies /api to backend on API_PORT
# In prod: the Rust binary serves everything on APP_PORT directly
APP_PORT     ?= 9999
API_PORT     ?= 8999
DB_PATH      := note.db
WEB_DIR      := web
PLIST_NAME   := com.scott.note
PLIST_SRC    := $(PLIST_NAME).plist
PLIST_DEST   := $(HOME)/Library/LaunchAgents/$(PLIST_SRC)
LOG_DIR      := $(HOME)/Library/Logs

# Load .env if present (Make-native VAR=value format)
-include .env
export

# Use direnv to load .envrc for commands that need API keys
DIRENV := $(shell command -v direnv 2>/dev/null)
ifdef DIRENV
  ENV_EXEC := direnv exec .
else
  ENV_EXEC :=
endif

# ── Development ──────────────────────────────────────────────
# Dev processes run in the background. PID files track them for stop/restart.

PID_DIR      := .pids
DEV_API_PID  := $(PID_DIR)/dev-api.pid
DEV_WEB_PID  := $(PID_DIR)/dev-web.pid
DEV_API_LOG  := $(LOG_DIR)/note.dev-api.log
DEV_WEB_LOG  := $(LOG_DIR)/note.dev-web.log

.PHONY: dev
dev: _ensure-cargo-watch _ensure-web-deps _ensure-ollama _ensure-env ## Start dev servers in background
	@mkdir -p $(PID_DIR)
	@# Stop any existing dev processes
	@if [ -f $(DEV_API_PID) ] && kill -0 $$(cat $(DEV_API_PID)) 2>/dev/null; then \
		kill $$(cat $(DEV_API_PID)) 2>/dev/null; sleep 1; \
	fi
	@if [ -f $(DEV_WEB_PID) ] && kill -0 $$(cat $(DEV_WEB_PID)) 2>/dev/null; then \
		kill $$(cat $(DEV_WEB_PID)) 2>/dev/null; sleep 1; \
	fi
	@# Start backend
	@NOTE_PORT=$(API_PORT) $(ENV_EXEC) cargo watch -x run -w src -w Cargo.toml -q \
		> $(DEV_API_LOG) 2>&1 & echo $$! > $(DEV_API_PID)
	@echo "Backend starting (pid $$(cat $(DEV_API_PID)))..."
	@for i in $$(seq 1 60); do \
		sleep 1; \
		curl -sf http://localhost:$(API_PORT)/api/status >/dev/null 2>&1 && break; \
	done
	@curl -sf http://localhost:$(API_PORT)/api/status >/dev/null 2>&1 || { \
		echo "Backend failed to start on port $(API_PORT)"; \
		cat $(DEV_API_LOG) | tail -20; \
		exit 1; \
	}
	@echo "Backend ready on :$(API_PORT)"
	@# Start frontend
	@cd $(WEB_DIR) && APP_PORT=$(APP_PORT) API_PORT=$(API_PORT) npx vite \
		> $(DEV_WEB_LOG) 2>&1 & echo $$! > $(DEV_WEB_PID)
	@echo "Frontend starting (pid $$(cat $(DEV_WEB_PID)))..."
	@sleep 2
	@echo ""
	@echo "Dev servers running in background:"
	@echo "  App:  http://localhost:$(APP_PORT)"
	@echo "  API:  http://localhost:$(API_PORT)"
	@echo ""
	@echo "  make dev-logs   Tail logs"
	@echo "  make dev-stop   Stop dev servers"
	@echo "  make dev-status Check health"

.PHONY: dev-stop
dev-stop: ## Stop dev servers
	@if [ -f $(DEV_API_PID) ]; then \
		kill $$(cat $(DEV_API_PID)) 2>/dev/null && echo "Stopped backend" || echo "Backend not running"; \
		rm -f $(DEV_API_PID); \
	fi
	@if [ -f $(DEV_WEB_PID) ]; then \
		kill $$(cat $(DEV_WEB_PID)) 2>/dev/null && echo "Stopped frontend" || echo "Frontend not running"; \
		rm -f $(DEV_WEB_PID); \
	fi
	@# Clean up any orphan processes on dev ports
	@lsof -ti :$(API_PORT) | xargs kill 2>/dev/null || true
	@lsof -ti :$(APP_PORT) | xargs kill 2>/dev/null || true

.PHONY: dev-logs
dev-logs: ## Tail dev server logs
	@tail -f $(DEV_API_LOG) $(DEV_WEB_LOG)

.PHONY: dev-status
dev-status: ## Check dev server status
	@echo "=== Backend ==="
	@if [ -f $(DEV_API_PID) ] && kill -0 $$(cat $(DEV_API_PID)) 2>/dev/null; then \
		echo "Running (pid $$(cat $(DEV_API_PID)))"; \
	else \
		echo "Not running"; \
	fi
	@echo ""
	@echo "=== Frontend ==="
	@if [ -f $(DEV_WEB_PID) ] && kill -0 $$(cat $(DEV_WEB_PID)) 2>/dev/null; then \
		echo "Running (pid $$(cat $(DEV_WEB_PID)))"; \
	else \
		echo "Not running"; \
	fi
	@echo ""
	@echo "=== Health ==="
	@curl -sf http://localhost:$(API_PORT)/api/status | python3 -m json.tool 2>/dev/null || echo "API not responding"

.PHONY: run
run: _ensure-env ## Run backend once (no reload)
	NOTE_PORT=$(API_PORT) $(ENV_EXEC) cargo run

# ── Building ─────────────────────────────────────────────────

.PHONY: build
build: build-web build-api ## Build everything for production

.PHONY: build-api
build-api: ## Build release binary
	cargo build --release

.PHONY: build-web
build-web: _ensure-web-deps ## Build frontend for production
	cd $(WEB_DIR) && npx vite build

# ── Testing ──────────────────────────────────────────────────

.PHONY: test
test: test-api test-web ## Run all tests

.PHONY: test-api
test-api: ## Run backend tests
	cargo test

.PHONY: test-web
test-web: _ensure-web-deps ## Run frontend tests
	cd $(WEB_DIR) && bun run test --run

.PHONY: test-web-watch
test-web-watch: _ensure-web-deps ## Run frontend tests in watch mode
	cd $(WEB_DIR) && bun run test

.PHONY: test-web-ui
test-web-ui: _ensure-web-deps ## Run frontend tests with browser UI
	cd $(WEB_DIR) && bun run test:ui

# ── Code Quality ─────────────────────────────────────────────

.PHONY: check
check: ## Run all checks (clippy, lint, typecheck, bug scan)
	@$(MAKE) -j3 check-api check-web check-ubs

.PHONY: check-api
check-api: ## Run clippy on backend
	cargo clippy -- -D warnings

.PHONY: check-web
check-web: _ensure-web-deps ## Lint + typecheck frontend
	cd $(WEB_DIR) && bun run lint && npx tsc -b --noEmit

.PHONY: check-ubs
check-ubs: ## Run UBS bug scanner (critical issues only)
	ubs . --no-auto-update

.PHONY: scan
scan: ## Run UBS bug scanner with verbose output
	ubs -v . --no-auto-update

.PHONY: fmt
fmt: ## Format all code
	cargo fmt
	cd $(WEB_DIR) && npx prettier --write 'src/**/*.{ts,tsx,css}'

# ── Service (macOS LaunchAgent) ──────────────────────────────
# Config lives in .envrc (secrets) + com.scott.note.plist (launchd).
# bin/note-service wrapper sources .envrc before launching the binary.

.PHONY: install
install: build ## Build and install as macOS service
	@mkdir -p "$(HOME)/Library/LaunchAgents"
	@cp "$(PLIST_SRC)" "$(PLIST_DEST)"
	@launchctl bootout gui/$$(id -u) "$(PLIST_DEST)" 2>/dev/null || true
	@launchctl bootstrap gui/$$(id -u) "$(PLIST_DEST)"
	@echo "Service installed. Access at http://localhost:$(APP_PORT)"

.PHONY: uninstall
uninstall: ## Stop and remove macOS service
	@launchctl bootout gui/$$(id -u) "$(PLIST_DEST)" 2>/dev/null || true
	@rm -f "$(PLIST_DEST)"
	@echo "Service uninstalled."

.PHONY: start
start: ## Start the service
	@launchctl kickstart gui/$$(id -u)/$(PLIST_NAME)

.PHONY: stop
stop: ## Stop the service
	@launchctl kill SIGTERM gui/$$(id -u)/$(PLIST_NAME)

.PHONY: restart
restart: ## Restart the service
	@launchctl kickstart -k gui/$$(id -u)/$(PLIST_NAME)
	@echo "Service restarted."

.PHONY: deploy
deploy: build ## Build and restart the production service
	@cp "$(PLIST_SRC)" "$(PLIST_DEST)"
	@launchctl bootout gui/$$(id -u) "$(PLIST_DEST)" 2>/dev/null || true
	@launchctl bootstrap gui/$$(id -u) "$(PLIST_DEST)"
	@echo "Deployed and restarted."

.PHONY: status
status: ## Show service status and health
	@echo "=== Service ==="
	@launchctl print gui/$$(id -u)/$(PLIST_NAME) 2>/dev/null | head -20 || echo "Service not loaded"
	@echo ""
	@echo "=== Health ==="
	@curl -sf http://localhost:$(APP_PORT)/api/status | python3 -m json.tool 2>/dev/null || echo "Not responding on port $(APP_PORT)"

.PHONY: logs
logs: ## Tail service logs
	@tail -f "$(LOG_DIR)/note.stdout.log" "$(LOG_DIR)/note.stderr.log"

# ── Database ─────────────────────────────────────────────────

.PHONY: db-shell
db-shell: ## Open SQLite shell on the database
	sqlite3 $(DB_PATH)

BACKUP_PLIST_NAME := com.scott.note-backup
BACKUP_PLIST_SRC  := $(BACKUP_PLIST_NAME).plist
BACKUP_PLIST_DEST := $(HOME)/Library/LaunchAgents/$(BACKUP_PLIST_SRC)
BACKUP_DIR        := $(HOME)/Library/Mobile Documents/com~apple~CloudDocs/__Business__/--Data-Migration--/nixonnote

.PHONY: db-backup
db-backup: ## Run a backup now (to iCloud)
	@./bin/note-backup

.PHONY: backup-install
backup-install: ## Install automated backup schedule (twice daily)
	@mkdir -p "$(HOME)/Library/LaunchAgents"
	@cp "$(BACKUP_PLIST_SRC)" "$(BACKUP_PLIST_DEST)"
	@launchctl bootout gui/$$(id -u) "$(BACKUP_PLIST_DEST)" 2>/dev/null || true
	@launchctl bootstrap gui/$$(id -u) "$(BACKUP_PLIST_DEST)"
	@echo "Backup schedule installed (8am + 8pm daily)"

.PHONY: backup-uninstall
backup-uninstall: ## Remove automated backup schedule
	@launchctl bootout gui/$$(id -u) "$(BACKUP_PLIST_DEST)" 2>/dev/null || true
	@rm -f "$(BACKUP_PLIST_DEST)"
	@echo "Backup schedule removed"

.PHONY: backup-status
backup-status: ## Show backup status and recent backups
	@echo "=== Schedule ==="
	@launchctl print gui/$$(id -u)/$(BACKUP_PLIST_NAME) 2>/dev/null | head -5 || echo "Not scheduled"
	@echo ""
	@echo "=== Recent Backups ==="
	@ls -lht "$(BACKUP_DIR)"/nixonnote-*.tar.gz 2>/dev/null | head -10 || echo "No backups found"
	@echo ""
	@echo "=== Monthly Backups ==="
	@ls -lht "$(BACKUP_DIR)"/nixonnote-monthly-*.tar.gz 2>/dev/null || echo "No monthly backups"

# ── Setup & Cleanup ─────────────────────────────────────────

.PHONY: setup
setup: ## First-time setup: install deps, create .env
	@echo "Installing Rust dependencies..."
	@cargo check
	@echo ""
	@echo "Installing frontend dependencies..."
	@cd $(WEB_DIR) && bun install
	@echo ""
	@if [ ! -f .env ]; then \
		cp .env.example .env; \
		echo "Created .env from .env.example - edit it to add your API keys"; \
	else \
		echo ".env already exists"; \
	fi
	@echo ""
	@cargo watch --version >/dev/null 2>&1 || { \
		echo "Installing cargo-watch for auto-reload..."; \
		cargo install cargo-watch; \
	}
	@echo ""
	@echo "Setup complete! Run 'make dev' to start developing."

.PHONY: clean
clean: ## Remove build artifacts
	cargo clean
	rm -rf $(WEB_DIR)/dist $(WEB_DIR)/node_modules/.vite

.PHONY: nuke
nuke: clean ## Remove everything (build artifacts + node_modules)
	rm -rf $(WEB_DIR)/node_modules

.PHONY: kill
kill: dev-stop ## Kill all processes on dev ports
	@lsof -ti :$(API_PORT) | xargs kill -9 2>/dev/null || true
	@lsof -ti :$(APP_PORT) | xargs kill -9 2>/dev/null || true
	@echo "Killed processes on ports $(API_PORT) and $(APP_PORT)"

# ── Internal ─────────────────────────────────────────────────

.PHONY: _ensure-env
_ensure-env:
	@[ -f .env ] || [ -f .envrc ] || { \
		echo "No .env or .envrc file found. Run 'make setup' or 'cp .env.example .env' and add your API keys."; \
		exit 1; \
	}

.PHONY: _ensure-ollama
_ensure-ollama:
	@curl -sf http://localhost:11434/api/tags >/dev/null 2>&1 || { \
		echo "Ollama not running. Starting via brew services..."; \
		brew services start ollama; \
		for i in 1 2 3 4 5; do \
			sleep 1; \
			curl -sf http://localhost:11434/api/tags >/dev/null 2>&1 && break; \
		done; \
		curl -sf http://localhost:11434/api/tags >/dev/null 2>&1 || { \
			echo "Failed to start Ollama. Try: brew install ollama"; \
			exit 1; \
		}; \
		echo "Ollama started."; \
	}

.PHONY: _ensure-cargo-watch
_ensure-cargo-watch:
	@cargo watch --version >/dev/null 2>&1 || { \
		echo "cargo-watch not found. Install with: cargo install cargo-watch"; \
		echo "Or run 'make setup' for full setup."; \
		exit 1; \
	}

.PHONY: _ensure-web-deps
_ensure-web-deps:
	@[ -d $(WEB_DIR)/node_modules ] || { echo "Installing web deps..."; cd $(WEB_DIR) && bun install; }

# ── Help ─────────────────────────────────────────────────────

.PHONY: help
help: ## Show this help
	@printf '\nUsage: make \033[36m<target>\033[0m\n\n'
	@awk 'BEGIN {FS = ":.*##"} /^[a-zA-Z_-]+:.*##/ { \
		printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2 \
	}' $(MAKEFILE_LIST)
	@echo ""
	@printf '\033[1mDev workflow:\033[0m\n'
	@printf '  \033[36mmake dev\033[0m          Start dev servers in background\n'
	@printf '  \033[36mmake dev-logs\033[0m     Tail dev logs\n'
	@printf '  \033[36mmake dev-stop\033[0m     Stop dev servers\n'
	@printf '  \033[36mmake dev-status\033[0m   Check dev health\n'
	@printf '\n'
	@printf '\033[1mProd workflow:\033[0m\n'
	@printf '  \033[36mmake deploy\033[0m       Build + restart launchd service\n'
	@printf '  \033[36mmake status\033[0m       Check prod service health\n'
	@printf '  \033[36mmake logs\033[0m         Tail prod logs\n'
	@echo ""
