# V1 Testing Plan

## Scope

This plan verifies the current [system design](system-design.md). Native backend and MCP behavior are verified before container construction. Live mutations require disposable Gmail data.

## Unit and component tests

### Happy routes

- Decode nested Gmail MIME bodies, prefer plain text, fall back from HTML, and collect attachment descriptors.
- Apply SQLite migrations and complete configuration CRUD.
- Persist a message summary and reuse it without another provider request.
- Persist an unchanged thread summary and reuse it without another provider request.
- Add one message to a thread, reuse prior message summaries, call the provider once for the new message, and call it once for the changed thread.
- Select Groq after an OpenAI failure and parse the strict summary response.
- Retry transport failures and HTTP `408`, `429`, and `5xx`, honor `Retry-After`, reject retries for other `4xx` responses, and fall back after retry exhaustion.
- Enforce the configured provider concurrency cap across shared summarizer clones.
- Serialize tool successes, summary states, and failures to their documented JSON shapes.
- Serialize full reads by default and exclude content, headers, snippets, labels, and attachments from summaries-only reads.
- Preserve Gmail pagination metadata in summary-only message pages and exclude full-message fields from every page item.

### Bad routes

- Reject empty identifiers, invalid page sizes, no-op label modifications, invalid Basic authentication, duplicate configuration names, and missing records.
- Prevent source filenames and identifiers from controlling attachment paths.
- Preserve Gmail content and report unavailable summary state after SQLite or provider-chain failure.
- Return `storage_error` for attachment filesystem failure without attachment bytes.

## Native MCP smoke tests

### Credential-free

- Start the native service against temporary SQLite and deterministic Gmail/provider boundaries.
- Initialize MCP and verify discovery exposes exactly the 12 documented tools.
- Invoke every tool and verify its typed success or structured failure route.
- Verify `list_messages.summaries_only` is optional, defaults to `false`, and does not add a thirteenth tool.

### Credentialed

- List and read Gmail data from the authenticated mailbox without printing message bodies.
- Fetch a real thread and verify every message summary plus the overall thread summary.
- Repeat message and thread reads in summaries-only mode and verify no full-message fields are returned.
- Request a small summary-only message page and verify its items and Gmail pagination metadata.
- Fetch the same thread again and verify SQLite reuse from safe provider-call metadata.
- Exercise changed-thread behavior with a deterministic fixture unless a disposable live thread is supplied.
- Exercise label, archive, spam, and attachment operations only on disposable Gmail data.

## Container verification

- Build the image only after native functional smoke tests pass.
- Resolve Compose configuration and start the service with prepared host artifacts.
- Repeat MCP discovery and tool routes through the container endpoint.
- Restart the container and verify SQLite, OAuth-token, and attachment persistence.

Local/component, live Gmail, and container results are reported as separate gates.
