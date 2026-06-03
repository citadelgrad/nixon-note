# Security Policy

NixonNote is designed for local-first, self-hosted use.

## Supported use

- Run on localhost, a private LAN, or a private overlay network such as Tailscale.
- Set `NOTE_TOKEN` before exposing the API beyond localhost.
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
