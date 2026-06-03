#!/usr/bin/env bash
#
# Setup script for AI features (Osaurus, Ollama, Claude)
#

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

echo "=== Note AI Services Setup ==="
echo ""

# 1. Check Osaurus
echo "1. Voice Transcription (Osaurus)"
if pgrep -f "osaurus.*8080" > /dev/null; then
    log_info "Osaurus is running on port 8080"
else
    log_warn "Osaurus is NOT running"
    echo "   Start with: osaurus --port 8080 &"
fi
echo ""

# 2. Check Ollama
echo "2. Embeddings (Ollama)"
if command -v ollama &> /dev/null; then
    if ollama list | grep -q "nomic-embed-text"; then
        log_info "Ollama and nomic-embed-text model are ready"
    else
        log_warn "Ollama installed but nomic-embed-text model not found"
        echo "   Pull with: ollama pull nomic-embed-text"
    fi
else
    log_warn "Ollama not installed"
    echo "   Install from: https://ollama.ai/"
fi
echo ""

# 3. Check Claude API
echo "3. Auto-tagging (Claude API)"
if grep -q "ANTHROPIC_API_KEY" ~/Library/LaunchAgents/com.scott.note.plist | grep -v "<!--"; then
    log_info "Claude API key is configured"
else
    log_warn "Claude API key not configured"
    echo "   1. Get API key from: https://console.anthropic.com/"
    echo "   2. Edit com.scott.note.plist and uncomment ANTHROPIC_API_KEY"
    echo "   3. Run: ./service.sh reload"
fi
echo ""

echo "=== Current Status ==="
./service.sh status | tail -20
