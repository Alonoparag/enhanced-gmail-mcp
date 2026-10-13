# System Design

## Functional Requirements

- **FR1:** The system must let an MCP caller retrieve paged Gmail message identifiers or summary-only items and retrieve Gmail threads, including inbox-only retrieval and Gmail search-based filtering.
- **FR2:** The system must let an MCP caller retrieve and create labels and add or remove labels from emails.
- **FR3:** The system must let an MCP caller archive emails and mark emails as spam.
- **FR4:** The system must let an MCP caller download email attachments.
- **FR5:** The system must return each read Gmail message as JSON serialized from Rust-owned structures, with a generated message summary. The caller may request summaries-only single-message or paged-list results that omit message content.
- **FR6:** The system must let a local operator create, retrieve, update, and delete server configurations through an API.
- **FR7:** The system must return each read Gmail thread with a generated thread summary and generated summaries for its constituent messages. The caller may request a summaries-only result that omits constituent message content. Re-reading a changed thread must include newly arrived messages.

## Technical Requirements

- **TR1:** The system must run as a Rust service in a Docker Compose environment and access Gmail through `google-gmail1` using OAuth 2.0 user authorization and operation-specific scopes.
- **TR2:** SQLite must own durable server configuration and summary-cache state.
- **TR3:** The service must expose MCP over Streamable HTTP so clients outside its container can connect.
- **TR4:** Public tool inputs, successful outputs, and failures must use typed Rust structures with discoverable JSON schemas and Serde JSON serialization.
- **TR5:** Email reading must extract textual content from Gmail MIME payloads and request message and thread summaries from Groq `openai/gpt-oss-20b` with low reasoning. If Groq is unavailable, the service must use OpenAI `gpt-6-luna` with reasoning disabled. A summarization failure must preserve email content and expose summary status separately.
- **TR6:** Credentials, OAuth tokens, email content, summaries, and attachment data must not be written to logs. Persistent credentials and SQLite data must survive container recreation through mounted storage.
- **TR7:** The local HTTP interfaces may use Basic authentication.
- **TR8:** Summary identities must include the summary contract version, configured model chain, and source content. Thread-summary identities must additionally include the ordered identities of their constituent message summaries. An unchanged identity must reuse its stored summary without a provider call.
- **TR9:** Host-side startup preparation must validate configured provider keys in ignored mode-`0600` files and may export missing files from macOS Keychain. The Linux service reads the files and does not access Keychain directly.
- **TR10:** Summary-provider calls must use a global bounded concurrency limit. Transient transport failures and HTTP `408`, `429`, and `5xx` responses must use bounded retries that honor `Retry-After` and otherwise apply capped exponential backoff with jitter. Other client errors and invalid successful responses are not retried.

## System Interface

### MCP

The service exposes Streamable HTTP MCP at `/mcp` with these tools:

- `list_messages`: lists one Gmail page as message identifiers by default or summary-only items when requested, using Gmail query syntax, optional label filters, pagination, and an inbox-only option.
- `get_message`: reads one message and returns either the full message representation or its identifiers and summary state.
- `get_thread`: reads all messages in one thread and returns either the full thread representation or its identifiers and summary states.
- `list_labels`, `create_label`, `update_label`, and `delete_label`: manage Gmail labels.
- `modify_message_labels` and `modify_thread_labels`: atomically add and remove label identifiers.
- `archive_message`: removes `INBOX` from a message.
- `mark_message_spam`: adds `SPAM` to a message.
- `download_attachment`: writes one Gmail attachment beneath the configured attachment root and returns its metadata.

Each tool operates on the authenticated Gmail user `me`. Tool descriptions and input schemas are discoverable through MCP.

### Configuration HTTP API

The operator API is available at `/configurations` and `/configurations/{id}`:

- `POST /configurations` creates a configuration.
- `GET /configurations` lists configurations.
- `GET /configurations/{id}` retrieves one configuration.
- `PUT /configurations/{id}` replaces its mutable fields.
- `DELETE /configurations/{id}` deletes it.

These routes require HTTP Basic authentication. They never accept or return Google OAuth credentials, OAuth tokens, the OpenAI API key, or Basic-auth credentials.

### Host preparation

Before native or container startup, the operator invokes one host command that verifies the OAuth token cache and configured mode-`0600` provider files, then copies the OAuth cache into the selected runtime data location. It may populate an absent provider file from macOS Keychain. The command fails before startup when required credentials are unavailable and never prints credential values.

## Data Flow

### Message reads

1. MCP validates the typed request.
2. The Gmail client executes the operation with its required scope.
3. The service traverses nested MIME parts, prefers `text/plain`, falls back to text extracted from `text/html`, and collects attachment descriptors.
4. The service derives a summary cache key from the message content, summary contract version, and model.
5. A SQLite hit supplies the summary. On a miss, the service calls the configured provider chain and persists a successful response.
6. The service selects the full or summaries-only response shape requested by the caller. Summary failure remains visible through summary state.

### Message list reads

1. MCP validates the typed request and retrieves one Gmail list page.
2. The default route returns that page's message identifiers and pagination metadata.
3. For a summaries-only request, the service reads and summarizes each listed message through the message-read cache and provider flow.
4. The service returns summary-only items with the original Gmail `next_page_token` and `result_size_estimate`; it creates no additional cursor.

### Thread reads

1. MCP validates the typed request and retrieves the complete Gmail thread.
2. Each ordered message follows the message-read flow, reusing unchanged message summaries.
3. If every message summary is available, the service derives the thread identity from the ordered message-summary identities.
4. A SQLite hit supplies the thread summary. On a miss, the provider chain summarizes the ordered message summaries and persists the result.
5. A newly arrived message changes the thread identity; existing message summaries remain reusable and the thread summary is regenerated.
6. The service selects the full or summaries-only response shape requested by the caller, with unavailable summary state for each affected summary.

### Gmail mutations

1. MCP validates identifiers and label changes.
2. The Gmail client executes the narrow label or mutation operation.
3. The service returns the resulting Gmail identifiers and label identifiers.

### Attachment download

1. MCP validates the message and attachment identifiers.
2. The Gmail client retrieves and decodes the attachment bytes.
3. The service chooses a generated filename, creates it beneath the attachment root, and records no caller-controlled path segment.
4. The service returns metadata and the server-owned path without returning attachment bytes.

### Configuration CRUD

1. The HTTP layer authenticates the operator with Basic authentication.
2. The request is validated as a typed configuration contract.
3. The repository executes the SQLite transaction.
4. The API returns the stored non-secret representation or a structured failure.

## Data Contracts

### Shared contracts

- `Failure`: `code`, `message`, `retryable`, and `context`. Context is a bounded map of safe identifiers and field names.
- `MessageRef`: Gmail `id` and optional `thread_id`.
- `Page<T>`: `items`, optional `next_page_token`, and optional `result_size_estimate`.

### Message contracts

- `ListMessagesInput`: optional Gmail `query`, `label_ids`, optional `inbox_only`, optional `include_spam_trash`, optional `max_results` from 1 through 100, optional `page_token`, and optional `summaries_only` defaulting to `false`.
- `GetMessageInput`: non-empty `id` and optional `summaries_only`, defaulting to `false`.
- `GetThreadInput`: non-empty `id` and optional `summaries_only`, defaulting to `false`.
- `GmailMessage`: `id`, optional `thread_id`, `label_ids`, optional `snippet`, optional Gmail `internal_date`, selected headers (`from`, `to`, `cc`, `subject`, `date`), extracted `content`, attachment descriptors, and `summary`.
- `GmailThread`: `id`, ordered `messages`, and `summary`.
- `GmailMessageSummary`: `id`, optional `thread_id`, and `summary`.
- `MessagePage`: `MessageRef` items, optional Gmail `next_page_token`, and optional `result_size_estimate`.
- `MessageSummaryPage`: `GmailMessageSummary` items with the same pagination metadata.
- `GmailThreadSummary`: `id`, ordered `GmailMessageSummary` values, and `summary`.
- Message and thread read outputs use their full contracts when `summaries_only` is false and their summary contracts when it is true. Message listing uses `MessagePage` when false and `MessageSummaryPage` when true.
- `AttachmentDescriptor`: Gmail attachment `id`, optional source `filename`, optional MIME type, and optional byte `size`.
- `SummaryResult`: `status` (`generated`, `cached`, or `unavailable`), optional `text`, model name, and optional safe failure code.

### Label and mutation contracts

- Label create and update inputs contain a non-empty `name` plus optional Gmail message-list and label-list visibility values.
- Label outputs contain `id`, `name`, optional label kind, and optional visibility values.
- Label modification inputs contain a non-empty message or thread `id` and `add_label_ids`/`remove_label_ids`; at least one list must be non-empty.
- Archive and spam inputs contain a non-empty message `id`.
- Mutation outputs contain the resulting Gmail `id`, optional `thread_id`, and `label_ids`.

### Attachment contract

- `DownloadAttachmentInput`: non-empty `message_id` and `attachment_id`.
- `DownloadedAttachment`: message and attachment identifiers, generated filename, server-owned path, and byte size.

### Configuration contract

- A configuration contains an integer `id`, unique non-empty `name`, optional Gmail query, `inbox_only`, `include_spam_trash`, `max_results` from 1 through 100, and creation/update timestamps.
- Create and replace requests contain the mutable fields and never contain secrets.

## Structured Failures

Public failures use stable codes:

- `invalid_request`: input or contract validation failed.
- `unauthorized`: Basic authentication is absent or invalid.
- `not_found`: the requested local or Gmail resource does not exist.
- `conflict`: a uniqueness or state constraint failed.
- `gmail_error`: Gmail rejected or could not complete an operation.
- `storage_error`: SQLite or filesystem work failed.
- `summary_error`: Summary cache or provider-chain work failed; full reads preserve message content while summaries-only reads and list items continue to omit it. A list-item summary failure is returned as unavailable summary state rather than failing the page.
- `internal_error`: an unclassified internal operation failed.

Failures expose a safe client message and retryability. Context may contain only bounded identifiers or field names. Provider diagnostics record the provider model, status or transport category, attempt, and retry delay. Logs omit credentials, authorization headers, OAuth tokens, message content, summaries, request and response bodies, raw Gmail payloads, and attachment bytes.

## Persistence

- SQLite owns configuration records and reconstructable message and thread summaries, and runs versioned migrations at startup. The database resides on a mounted Compose volume. Cache loss changes latency, not Gmail correctness.
- The Google OAuth token cache resides on a mounted Compose volume. Initial installed-app consent is completed with a host-assisted bootstrap before unattended container startup.
- Downloaded attachments reside beneath a mounted attachment root. Generated filenames prevent traversal and collisions.
- OpenAI and Groq API keys reside in ignored mode-`0600` host files. The host helper validates those files and may populate missing files from Apple Keychain; the container reads only the mounted secret paths.
- Basic-auth credentials enter the container through environment or secret configuration and are not stored by configuration CRUD.

## Out of Scope

- Sending Gmail messages.
- Creating or managing persistent Gmail filters.
- Unattended completion of first-time Google OAuth consent inside the Compose service.
- Treating summary-cache records or downloaded attachments as authoritative mailbox storage.
