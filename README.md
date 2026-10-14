<div align="center">

# Enhanced Gmail MCP

**A privacy-conscious Gmail server for Model Context Protocol clients.**

Read and organize Gmail, download attachments, and generate cached summaries through a typed Rust service.

[![Rust](https://img.shields.io/badge/Rust-1.88%2B-000000?logo=rust)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-Streamable_HTTP-6f42c1)](https://modelcontextprotocol.io/)
[![Gmail API](https://img.shields.io/badge/Gmail-API-EA4335?logo=gmail&logoColor=white)](https://developers.google.com/gmail/api)
[![Docker Compose](https://img.shields.io/badge/Docker-Compose-2496ED?logo=docker&logoColor=white)](https://docs.docker.com/compose/)

</div>

## What it does

- Searches and reads Gmail messages and complete threads.
- Returns full typed results or privacy-minimized summary-only pages.
- Creates, updates, deletes, and applies Gmail labels.
- Archives messages, marks spam, and downloads attachments to server-owned storage.
- Generates summaries with Groq `openai/gpt-oss-20b`, falling back to OpenAI `gpt-6-luna` when Groq is unavailable.
- Reuses content-addressed summaries from SQLite to avoid unnecessary provider calls.
- Exposes a local configuration API alongside the MCP endpoint.

The MCP server uses Streamable HTTP at `http://localhost:3000/mcp`. The configuration API is available at `/configurations` and `/configurations/{id}`.

## Design at a glance

```text
MCP client ──► Rust / Axum service ──► Gmail API
                         │
                         ├──► Groq ──fallback──► OpenAI
                         │
                         └──► SQLite cache + configuration
```

The [system design](docs/system-design.md) owns the current requirements and contracts. The active [implementation plan](docs/implementation-plan-v1.md) and [testing plan](docs/testing-plan-v1.md) track delivery and verification.

## Prerequisites

- Rust 1.88 or newer
- Docker with Compose
- A Google Cloud Desktop OAuth client with the Gmail API enabled
- A Groq API key
- Optional OpenAI API key for provider fallback

## Configure Gmail OAuth

1. Download the Google Desktop OAuth client JSON to `.secrets/client_secret.json`.
2. Restrict the file and complete the installed-app consent flow:

```sh
mkdir -p .secrets
chmod 700 .secrets
chmod 600 .secrets/client_secret.json
cargo run --bin oauth_bootstrap
```

The bootstrap writes `token_cache.json` at the repository root by default. Runtime storage preserves the cache across container recreation, allowing unattended token refresh after initial consent.

## Configure summary providers

Store provider keys in ignored, mode-`0600` files:

```sh
chmod 600 .secrets/groq_api_key
chmod 600 .secrets/openai_api_key  # optional fallback
cargo run --bin prepare_host -- .runtime-data both
```

`prepare_host` validates provider files and OAuth state without printing credential values. On macOS, it can export a missing provider file from Keychain. Use `groq` instead of `both` when OpenAI fallback is not configured.

## Run

Set local configuration-API credentials, then start the service:

```sh
export CONFIG_BASIC_USERNAME=operator
export CONFIG_BASIC_PASSWORD='replace-me'
docker compose up --build
```

Compose binds the service to `127.0.0.1:3000`, mounts persistent OAuth and SQLite state, and stores downloaded attachments in a named volume.

## Verify locally

```sh
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
CONFIG_BASIC_USERNAME=test CONFIG_BASIC_PASSWORD=test docker compose config
```

Live mutation and download tests require operator credentials and disposable Gmail data. Provider fallback is covered deterministically without intentionally failing a live provider.

## Security boundaries

- OAuth credentials, tokens, provider keys, runtime data, and downloaded attachments are ignored by Git.
- Secrets must use mode `0600`; `.secrets/` must use mode `0700`.
- The service never requests Gmail send scope.
- Logs omit credentials, authorization headers, message content, summaries, and attachment bytes.
- The MCP and configuration interfaces bind to loopback by default.

See the [system design](docs/system-design.md) for the complete interface, persistence, failure, and out-of-scope contracts.
