# Security Policy

NixonNote is designed for local-first, self-hosted use.

Embeddings and voice transcription are intentionally local: Ollama handles note/query embeddings and `simple_transcribe_rs` runs Whisper on-device. Provider API keys are only needed for optional higher-level features such as auto-organization, chat, and TTS.

## Supported use

- By default the server binds to `127.0.0.1` (`NOTE_HOST`) and is not reachable from your LAN.
- For remote access, prefer `tailscale serve`, which proxies from your tailnet to the loopback-only server without widening the bind address.
- If you set `NOTE_HOST` to a non-loopback address (for example `0.0.0.0`, to reach it from a private LAN or overlay network such as Tailscale directly), set `NOTE_TOKEN` first.
- Keep AI provider keys in environment variables or an untracked local env file. Never commit real keys.

## Public internet warning

Do not expose NixonNote directly to the public internet without:

1. `NOTE_TOKEN` set to a high-entropy secret.
2. A trusted reverse proxy or network boundary.
3. TLS termination.
4. Backups for `note.db`.

The service intentionally has a very small auth model for single-user/local deployments. It is not a multi-user SaaS auth system.

## Reporting vulnerabilities

Open a private GitHub security advisory if available, or contact the maintainer directly. Do not disclose active credential leaks publicly.
