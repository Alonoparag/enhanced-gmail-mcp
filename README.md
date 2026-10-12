# Enhanced Gmail MCP

A Rust Gmail service exposing Streamable HTTP MCP and a local configuration API. The [system design](docs/system-design.md) owns the V1 contract; the active [implementation](docs/implementation-plan-v1.md) and [testing](docs/testing-plan-v1.md) plans track delivery.

## V1 capabilities

- Query and read Gmail messages and threads with extracted text and attachment descriptors.
- Create, update, delete, and apply labels; archive messages and mark messages as spam.
- Download attachments into server-owned persistent storage.
- Summarize with Groq `openai/gpt-oss-20b`; OpenAI `gpt-6-luna` remains an optional primary provider when configured.
- Store non-secret server configurations and reconstructable summaries in SQLite.

MCP is served at `http://localhost:3000/mcp`. Configuration CRUD is served at `/configurations` and `/configurations/{id}`.

## Google OAuth bootstrap

Create a Google Cloud Desktop OAuth client, enable Gmail API access, and save its downloaded JSON as `.secrets/client_secret.json`. The file must use mode `0600`; Compose mounts it as a secret. Complete the installed-app consent flow on the host:

```sh
chmod 600 .secrets/client_secret.json
cargo run --bin oauth_bootstrap
```

The bootstrap defaults to `.secrets/client_secret.json` and writes `token_cache.json` at the repository root. Optional positional arguments can override both paths.

The named data volume preserves the token cache across container recreation. Token refresh is unattended after bootstrap.

The service requests read-only, label-management, or mailbox-modification scope for the operation being performed. It does not request send scope.

## API keys on macOS

Write the Groq key to the ignored `.secrets/groq_api_key` file, or place `GROQ_API_KEY=...` in `.secrets/.secrets`. Secret files must use mode `0600`. Then prepare the runtime directory. OpenAI configuration is optional:

```sh
mkdir -p .secrets
chmod 700 .secrets
# Write the key with your preferred secret manager or editor.
chmod 600 .secrets/groq_api_key
cargo run --bin prepare_host -- .runtime-data groq
```

`prepare_host` verifies the mode-safe OAuth cache and selected provider files, then copies the cache into the selected runtime data directory without printing credential values. If a selected provider file is absent, the command retains Keychain export as a compatibility fallback. Use `groq` for the default path; `openai` and `both` remain supported. When both are configured, OpenAI is primary and Groq is the fallback. `GROQ_MODEL` defaults to `openai/gpt-oss-20b`.

## Run with Compose

Set Basic-auth credentials without committing them:

```sh
export CONFIG_BASIC_USERNAME=operator
export CONFIG_BASIC_PASSWORD='replace-me'
docker compose up --build
```

Container build and startup follow successful native MCP smoke tests. Compose mounts `RUNTIME_DATA_DIR` (default `.runtime-data`) for SQLite and OAuth state and a named volume for attachments. The MCP host allowlist is explicit and includes loopback plus the Compose service authority.

## Local verification

```sh
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
CONFIG_BASIC_USERNAME=test CONFIG_BASIC_PASSWORD=test docker compose config
```

Live Gmail mutations, downloads, OAuth persistence, and provider fallback require operator credentials and disposable mailbox data; local compilation and fake-boundary tests do not replace those gates.
