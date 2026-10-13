# V1 Implementation Plan

## Scope

This plan implements the current [system design](system-design.md). The system design owns requirements, interfaces, contracts, failures, and persistence semantics.

## Phase 1: Native service foundation

Deliverables:

- Axum service with Streamable HTTP MCP at `/mcp`.
- Versioned SQLite migrations for configuration and summary state.
- Typed public contracts, startup validation, and safe logging boundaries.

Completion checks:

- Native startup exposes MCP initialization and repeatable migrations.
- Invalid configuration fails startup without exposing secret values.

## Phase 2: Gmail and summary tools

Deliverables:

- Paged identifier or summary-only message listing, full or summaries-only message and thread reads, MIME extraction, and attachment descriptors.
- Durable message and thread summaries with globally bounded provider concurrency, transient retries, OpenAI primary, and Groq fallback.
- Label management, label modification, archive, spam, and attachment-download tools.

Completion checks:

- All 12 discovered tools match the system-design contracts.
- Existing read requests remain compatible, while summaries-only requests exclude message content and metadata outside the summary contracts.
- Unchanged messages and threads reuse SQLite summaries.
- A changed thread reuses old message summaries, summarizes each new message, and regenerates its thread summary.
- Cache or provider failure preserves Gmail content and reports unavailable summary state.
- Summary-only message pages preserve Gmail pagination and report provider failure per item.
- Retryable provider failures honor `Retry-After` or capped exponential backoff with jitter before fallback.

## Phase 3: Host and configuration operations

Deliverables:

- Basic-authenticated configuration CRUD backed by SQLite.
- One host preparation command that validates the OAuth cache, exports selected Keychain provider secrets, and prepares a runtime data directory.

Completion checks:

- CRUD persists and never returns secret material.
- Host preparation creates only mode-safe credential artifacts and prints no credential values.

## Phase 4: Native functional verification

Deliverables:

- Unit and component coverage from the V1 testing plan.
- A direct native MCP smoke harness covering discovery and all 12 tools.
- Credentialed read-only Gmail and summary-cache smoke tests.

Completion checks:

- Formatting, compilation, tests, Clippy, and credential-free smoke checks pass.
- Read-only live tests prove message summaries, thread summaries, and unchanged-cache reuse.
- Mutation tools are exercised only against disposable Gmail data.

## Phase 5: Containerization and handoff

Deliverables:

- Dockerfile and Compose wiring for the verified native backend.
- Container and agent-driven MCP tests.
- Documentation consistency and maintainability reviews.

Completion checks:

- Container build begins only after Phase 4 passes.
- Compose validation, container smoke tests, persistence checks, and final reviews pass.
