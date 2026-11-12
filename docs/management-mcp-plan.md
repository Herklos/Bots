# Generated gateway management MCP

Status: implementation reference. The local management endpoint, generated tool/OpenAPI/coverage artifacts, scoped credentials, and Access-settings controls are implemented. See [the operating guide](management-mcp.md) for commands, tested behavior, and protocol limits. Remote OAuth is outside the initial local-only scope.

## Objective

Expose the gateway application API through an authenticated management MCP at `/mcp`, including Bot and group creation, updates and deletion. Generate its tool catalog and schemas from a shared API operation definition. Add a persistent Settings switch, off by default. Preserve the existing internal `/mcp/bot` endpoint and credentials.

## Initial repository findings

- Application routes are declared in `gateway/src/api/router/mod.rs`; system routes are merged from `gateway/src/platform/release/http.rs`.
- `/mcp/bot` is a local-only, bearer-authenticated internal tool server. Its implementation in `gateway/src/platform/mcp.rs` serves Bot-oriented tools, including Vault, children, memory, images and conditional Projects tools. It is not a management API.
- No OpenAPI/schema-generation integration was found in the gateway or workspace dependencies. Many handlers parse `Bytes` into `serde_json::Value`, so accurate schemas cannot be inferred from Rust signatures alone.
- HTTP handlers contain meaningful orchestration: Bot creation clamps runtime modes and wakes provisioning. Sharing only storage methods would lose these semantics.
- Settings updates merge arbitrary JSON. MCP must not forward unchecked patches that can alter its own authorization policy.
- Groups use the existing rooms domain and `/api/rooms` routes.

## Decisions

### One definition, generated surfaces

Introduce a typed operation registry used to register application HTTP routes and generate OpenAPI, MCP tool definitions, dispatch metadata and the parity manifest. Do not scrape Rust source, infer schemas from example responses, maintain a second handwritten MCP catalog, or provide an arbitrary HTTP-request tool.

Each operation declares:

- Stable operation ID, HTTP method/path and handler binding.
- Request schema for path, query and body, content types and size limits.
- Response schemas, status codes and error codes.
- Explicit MCP exposure classification, tool name and model-facing description.
- Required permissions, sensitive fields, feature gates and side-effect annotations.
- Binary/streaming adapter, if required.

Use shared typed DTOs and schema derivation where practical. Preserve existing JSON compatibility while incrementally replacing untyped inputs. For intentionally dynamic structures, supply explicit schemas and retain domain validation; never pretend an unconstrained object is a complete contract.

Evaluate a maintained Rust MCP SDK and OpenAPI/schema library against Axum, current released protocol versions, license, dependency footprint and existing error handling. Prove the choice with Bot create/update, group create, a query operation and a binary operation before broad conversion. Pin dependencies and supported protocol versions.

Generation should be deterministic and local, without an LLM or running gateway. Add a check mode that fails on stale committed OpenAPI/manifest artifacts. Runtime tool schemas should derive directly from registry metadata. Generated files must be marked as generated.

### Shared execution

Bind both transports to the same application operation functions, passing a transport-independent authenticated actor and request context. HTTP extraction and MCP argument decoding remain transport adapters. Preserve runtime caps, provisioning signals, feature gates, backend choice, validation, redaction, ordering and asynchronous behavior.

Extract application operations only where necessary; avoid a wholesale domain rewrite. Do not loop back through localhost HTTP, fabricate cookie sessions, or bypass access checks by calling storage directly. T3 RPC names stay in the harness adapter. Preserve default Bot model and computer-selection behavior.

### Complete coverage, explicit classification

Every mounted application method/path must have exactly one registry classification: generated tool, adapted tool, or infrastructure exclusion with rationale. Route registration must use the registry or the explicit infrastructure registration mechanism, so CI can detect new unclassified routes without source-text scraping.

| API area | Management coverage |
| --- | --- |
| Bots | List/get/create/update/delete, seen, main Bot selection |
| Providers/models | Providers, suggestions, Bot model get/update; preserve computer-dependent choices |
| Bot conversation | Chat read/send/stop, pending-request responses, voice |
| Attachments | List/create/download/delete with bounded binary adapters |
| Groups/rooms | List/get/create/update/delete, membership through existing update semantics, send message, seen |
| Threads | Previews, metadata, reactions, pins, replies, read, voice |
| Routines | Global and per-Bot list/upsert, enable, delete and run; preserve any distinct route semantics |
| Library/connectors | Read/save/delete, file retrieval, shelf, connector save/delete |
| Memory/context | Memory read/update, Bot lineage and code information |
| Computer | State, open/close, control and input |
| Vault | List/get/create/update/delete, JSON/CSV imports, CSV export |
| Settings/config | Read/update existing application settings and configuration, subject to control-plane restrictions below |
| Access | Status, password, network, restart, Tailscale and tunnel operations |
| Storage | Configuration, test/copy/restart, keys, Anytype pairing/confirmation/check/setup |
| System | Status/check, auto-update, notice dismissal, start update |
| Usage | Global and Bot usage, reports |
| Supporting data | Health/version and API icons via appropriate structured/binary adapter |

Use stable names such as `bots_create` and `groups_update`; keep wire IDs unchanged. Include tool schemas for all required fields, enums, nullability, patch semantics and limits. Preserve output envelopes rather than inventing incompatible simplified responses.

Explicit exclusions: login/logout/session endpoints (authentication infrastructure), static frontend assets, unrestricted `/t3/*` proxying, and transport-only streams. Existing Bot-only capabilities are not automatically management tools unless a corresponding application operation exists. Any streaming-only application capability needs a documented equivalent or an explicit gap, not silent omission. Audit merged and feature-gated routes before freezing the manifest.

### Activation and credentials

Persist `managementMcp.enabled`, default false when missing. Validate nested types and partial merges. Enable/disable without gateway restart. While disabled, `/mcp` returns a consistent unavailable response and does not reveal tool metadata. Authentication and activation are checked for every request and immediately before each operation starts. Work already started may finish; disabling is not rollback or cancellation.

The authenticated first-party Settings interface is the control plane for activation and credential issuance/revocation. Management MCP cannot enable itself, mint tokens, grant scopes, or change its own restrictions. Its settings tool may disable the feature: commit the setting and return success before rejecting future calls. Reject attempts to alter protected fields atomically rather than silently ignoring them.

Use separate credentials, never `.bot-mcp-token`, browser sessions or the T3 token. Require them even on passwordless installations. Store token hashes and metadata separately from settings exports; reveal plaintext once. Support labels, creation time, last-used metadata and individual revocation. Define secure file permissions, startup loading, rotation and failure behavior. Never log tokens or Vault payloads.

Permission classes: read, ordinary mutation, secret access, administration, computer control. An explicitly issued full-administration credential can access the full application catalog, except control-plane credential/activation restrictions. Scopes constrain execution as well as discovery. Secret-returning GETs are sensitive; GET does not imply harmless. Existing runtime and feature policies remain enforced.

Initial endpoint is loopback-only and rejects tunneled/proxied ingress. Validate peer, Host and Origin using an explicit trust policy; do not trust forwarded headers for client identity. Existing LAN/tunnel settings must not publish `/mcp`. Clearly document this local credential model without claiming remote OAuth compliance. Remote MCP is a separate extension requiring HTTPS, standard authorization and trusted ingress design.

### Settings UI

Add a section at the end of Access settings with the enable switch, endpoint URL, local-access explanation, token creation/revocation and a one-time secret display. Do not put credentials into copied URLs. Keep browser settings authentication unchanged. Prevent accidental re-display through snapshots, exports or logs.

Use `app/strings/en.json` and `fr.json`. Follow Exact client workflows for Contract/data-source changes and verify web and native builds where affected. Read status and save errors accurately; enabled does not imply a client is connected.

### Protocol and result semantics

Implement the required lifecycle, version negotiation, tool listing/calls, structured results, notifications and protocol errors for explicitly supported released versions. Advertise only implemented capabilities. Verify current requirements rather than copying the internal server's protocol-version echo or batch handling. Handle pagination of discovery if needed and enforce feature/scope filters consistently at call time.

Validate arguments before execution. Keep unknown tools, malformed protocol requests and application failures distinct. Map domain failures into stable machine-readable errors without exposing internals. Include read-only/destructive/idempotency annotations based on reviewed operation metadata, not method inference alone.

Preserve asynchronous application semantics for chat, routines, storage and updates. Return existing operation/thread IDs and status facilities; identify any missing status capability as a separately specified addition. Do not invent automatic retries for non-idempotent calls. Cancellation cannot promise to undo completed writes.

Binary operations require explicit adapters: bounded base64 for small inputs/outputs and authenticated expiring handles for larger objects if required. Define MIME type, filename, byte limits, expiry, cleanup and scope checks on retrieval. No arbitrary server paths or arbitrary URL fetching. Do not add a transfer subsystem unless existing limits/client support require it. Streaming computer transports require a precise equivalent, not a claim that a control tool reproduces a live stream.

Add bounded concurrency, payload/time limits and sanitized audit events (actor, tool, target, outcome, correlation ID). Network/storage/update actions can disconnect the caller; document acknowledgement and reconnection behavior and test it.

## Implementation sequence

1. Inventory all mounted routes and current input/output semantics, including binary and feature-gated behavior. Define operation IDs, classifications, permission matrix and acceptance manifest.
2. Prototype registry, schema generation and shared dispatch with representative operations. Verify client interoperability and select pinned SDK/schema dependencies. Approve architecture through code review, not a runtime feature rollout.
3. Add persistent off-by-default setting, control-plane credential endpoints and localized Settings UI. Establish isolation from internal Bot MCP and all external ingress paths.
4. Add transport/authentication/dispatch and generated core Bot/group/model/chat tools using shared application operations.
5. Convert remaining API areas to registry definitions and implement necessary binary/asynchronous adapters. Keep HTTP wire compatibility throughout.
6. Finish coverage enforcement, protocol/security/parity tests, backend and compatibility validation, client examples, documentation and release notes. Ship only after the full manifest is resolved.

Each phase is a reviewable change; do not label core CRUD alone as complete API parity.

## Validation and acceptance

- Registry checks: unique operation IDs/tool names, resolvable schemas, correct parameter placement/encoding, explicit classification of every mounted route, deterministic generation and stale-artifact failure.
- Protocol checks: released supported versions, lifecycle, discovery, schemas/results, errors, notifications, method/content-type handling and unsupported versions. Test real MCP clients plus a protocol conformance tool where available.
- Security checks: disabled default, runtime disable/re-enable via UI, revoked credentials, cross-token isolation, protected settings, secret redaction, local-only ingress, Host/Origin rejection and limits. A token cannot elevate itself through general settings/access/storage operations.
- Semantic parity: representative fixtures exercise the same operation through HTTP and MCP, compare persisted state and side effects, and verify runtime caps/provisioning. Cover each operation's success path and meaningful failure paths, not just tool registration.
- Complete flows: create/update/delete Bots and groups, membership changes, chat, routine runs, pending approvals, files/voice/CSV and sensitive administrative operations in disposable homes.
- Backends: filesystem and Anytype where supported, with failures and unavailability reported accurately. Never use the real user home for tests.
- Compatibility: old homes default disabled; unrelated settings patches preserve the flag; credential files are not exported; inspect older writers before claiming mixed-version preservation. Run `tests/compat/run.sh` in its default temporary workspace and report which versions actually ran.
- Repository checks: relevant Rust tests, Rust length limit, T3 boundary, Exact Contract/web/native checks as applicable, localized UI verification. Existing `/mcp/bot` remains operational and unchanged.

Definition of done: full route manifest resolved, generated schemas/catalog current, HTTP behavior preserved, no privilege-escalation path, Settings switch reliably gates management execution, and documented real-client CRUD flows pass.

## Review corrections incorporated

- Automatic generation requires explicit API schemas first; existing generic JSON handlers cannot supply them by inference.
- Generation now comes from the same registry that mounts routes, preventing a second catalog from drifting.
- Shared execution includes HTTP-handler orchestration, not only domain storage calls.
- Full parity is distinguished from infrastructure endpoints and internal Bot-only tools.
- General settings/access/storage tools cannot bypass activation or credential scopes.
- Existing Bot MCP is independent of the new switch and management credentials.
- Local bearer access is scoped honestly; remote authorization is not implicitly promised.
- Binary, asynchronous and streaming semantics are explicit acceptance work.
- Disabling blocks new work but does not imply rollback of work already executing.
- Unknown protocol versions, capabilities, batching and transport behavior require current specification verification.
