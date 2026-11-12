# Management MCP

hbots exposes its application API as MCP tools at `http://127.0.0.1:3780/mcp`.
Use the gateway's configured local port if it differs from 3780. The endpoint is
local-only, including when the application is available over LAN or a tunnel.

## Connect

1. Open Settings → Access and enable management MCP.
2. At the end of Access settings, choose **New access token**, enter a client
   name, and select its permissions. Read-only is selected initially; choose
   **Manage Bots and groups** to use the creation/update examples below.
3. Choose **Create access token**, then **Copy token**. Choose **Done** to hide
   the one-time result. The token cannot be retrieved later.
4. Configure a Streamable HTTP MCP client with the endpoint and an
   `Authorization: Bearer <token>` header.

Example client configuration (the client must support custom HTTP headers):

```json
{
  "mcpServers": {
    "hbots": {
      "url": "http://127.0.0.1:3780/mcp",
      "headers": { "Authorization": "Bearer <your-token>" }
    }
  }
}
```

Choose permissions when creating a token. The default is read-only. Other
choices add Bot/group management, Vault access, computer control, or full
administration. Full administration includes storage, networking and updates.
Issue tokens only to clients you trust with the selected access. Revoke a token in Settings to reject its subsequent requests. Create a
replacement to rotate it. Token hashes live in
`bots/.botapp/management-mcp-credentials.json`, with mode 0600; ordinary settings
responses never include them. Credential management requires the normal
application authentication and is not exposed as an MCP tool.

Disable the switch to reject subsequent MCP requests. Already-started operations
may finish; disabling does not undo a write or stop a Bot. Only Settings/the
first-party settings API can change activation. The management settings tool can disable MCP, but cannot enable it or issue
credentials. Existing Bot tools at `/mcp/bot` are independent.
Old homes default to disabled without a migration.

## Tools and arguments

The [generated catalog](management-mcp-tools.json) is the complete tool reference.
Names use `hbots_` followed by the existing application handler name. Groups use
room operations because they share the same persisted model.

Each tool exposes an API request envelope:

- `path`: required URL parameter names and string values.
- `query`: optional query names and string values.
- `body`: the existing API's JSON request body.
- `bodyBase64`: alternative raw request bytes, mutually exclusive with `body`.
- `contentType`: defaults to `application/json`; select the API's expected media
  type when sending raw bytes.

Create a Bot:

```json
{"name":"hbots_create_bot","arguments":{"body":{"name":"Ada"}}}
```

Update it using the slug returned from creation:

```json
{"name":"hbots_save_bot","arguments":{"path":{"slug":"ada"},"body":{"name":"Ada Lovelace"}}}
```

Create a group using existing Bot slugs:

```json
{"name":"hbots_rooms_create","arguments":{"body":{"title":"Research","members":["ada","bob"]}}}
```

A successful result contains `structuredContent: {"status": 200, "body": ...}`
and equivalent text content. API JSON bodies retain their original shape.
Binary responses use `{contentType, bodyBase64, contentDisposition}` in `body`,
including JSON files downloaded as attachments. Validated image downloads and
computer frames also produce native MCP image content. Tool failures return
`isError: true` and `structuredContent.error` with a stable `code`, `message`,
optional HTTP `status`, and original structured `details`. Unknown tools and
malformed protocol requests use JSON-RPC errors. Authentication and transport
failures use HTTP status codes.

## Coverage and generation

`application_routes()` registers HTTP routes and derives MCP operation metadata
from the same method/handler declarations. Calls dispatch through that exact
application Router in-process, after management authentication. They do not
make loopback network requests, invent browser sessions, or bypass handler
validation. Runtime caps, provisioning, feature gates and backend behavior remain
those of the API. System routes use the same registry.

The catalog covers Bots, rooms, threads, messages, attachments, voice, routines,
library, connectors, memory, computer control, models/providers, usage, Vault,
settings, access, storage and system updates. Login/session/logout, static assets,
health probe, the internal Bot MCP, credential control plane and unrestricted
T3 proxy routes are infrastructure rather than management tools. Live T3
WebSocket streams are not converted into MCP streams.

Run `scripts/generate-mcp.sh` after changing routes. It produces:

- [MCP definitions](management-mcp-tools.json).
- [OpenAPI 3.1](gateway-openapi.json), with request fields, query/path parameters,
  binary content types and response contracts.
- [API coverage manifest](gateway-api-coverage.json), classifying every registered
  application operation and infrastructure route with its permission or reason.

Normal gateway tests fail when any artifact is stale. Both application and
infrastructure registration derive metadata from the same route declarations;
there is no source scraping or LLM-based generation. New mutation handlers need
an explicit request contract before catalog generation succeeds.

The canonical JSON Schemas describe API request fields and common response
records. Existing extensible domain/provider payloads remain explicitly dynamic,
and unknown API extension fields are preserved. MCP validates these contracts
with pinned `jsonschema` 0.61.1, including JSON passed through `bodyBase64`.
Cross-field business rules remain in the shared HTTP handlers. This avoids a
second implementation of provisioning, runtime limits or provider rules.

CSV import accepts CSV text directly in `body` and defaults to `text/csv`.
`bodyBase64` remains available for raw CSV bytes and other API request content.

## Transport and limits

The endpoint implements stateless JSON-response Streamable HTTP for protocol
versions 2025-06-18 and 2025-11-25. Unsupported initialization proposals receive
2025-06-18 as the server's supported alternative; unsupported explicit protocol
headers receive 400. GET/DELETE receive 405 because there is no session or SSE
stream. Notifications receive 202. JSON-RPC batches are rejected. Clients must
send `Content-Type: application/json` and a loopback Host/Origin.

Up to 16 requests execute concurrently. The MCP envelope is limited to 24 MiB,
API-specific input limits still apply, and responses are bounded at 16 MiB.
There are no temporary public download links. Calls time out after 60 seconds;
a timeout does not imply rollback. Do not blindly retry message sends, routine
runs, creation, imports or other mutations.

Administrative operations can disconnect the caller or restart the gateway.
Existing asynchronous API responses and identifiers are preserved. MCP does not
add a separate job scheduler, OAuth server, remote access or stdio process.

Token scopes are `read`, `write`, `secrets`, `computer`, and `admin`. They are
independent; `admin` permits the whole catalog. The Settings presets include
`read` with each specialized capability. Discovery and execution both enforce
permissions. Storage, access controls, system operations and settings mutations
require administration. Vault reads require secrets permission, and computer
reads require computer permission. Tokens from the initial implementation
without a `scopes` field retain their original administrative access. Invalid
scopes fail closed. Credential metadata includes creation and last-used times.

## Verification

Run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
bun scripts/check-contract.mjs
bun scripts/check-size.mjs
sh scripts/check-rs-length.sh
sh scripts/check-t3-boundary.sh
bun scripts/test-management-mcp.mjs
bun scripts/test-mcp-sdk.mjs
```

The SDK test uses official `@modelcontextprotocol/sdk` 1.32.2 with a frozen
fixture lockfile. It builds the real gateway, starts two processes sharing a
new temporary home, and tests discovery, Bot/group CRUD, binary/JSON files,
Vault/CSV, shared memory, scoped access, cross-process revocation and activation.
It removes its processes, test credentials, home and dependencies afterward.
CI is configured to run it on Linux and macOS. No real T3 thread or user home is used.

Focused Rust tests additionally cover chat/voice dispatch, pending approvals,
Bot routine creation/enabling/running/deletion with a recorded mock harness,
runtime caps, malformed protocol messages, argument schemas, image content,
Tailscale listener isolation, corrupt credential preservation and stale files.
The JSON-response transport is tested against the supported
[MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
and [HTTP transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
contracts. It keeps the existing Axum router rather than adding a second Rust
server framework; the official SDK exercises its wire compatibility.

The macOS client builds successfully; the browser test exercises the token form,
permissions, copy/dismiss/revoke, the configured server address and the default-off
switch. Live T3/Anytype account interaction and native app interaction were not
performed. Their application handlers are shared with HTTP, and existing adapter
unit tests remain in the full gateway suite.

The legacy shared-home harness passed 26 checks and failed four existing
`/api/projects` checks, a route absent from the pre-change gateway. A separate
SDK compatibility run used the existing pre-MCP 0.3.0 binary and verified shared
sessions and preservation of the new setting through an older writer:

```sh
HBOTS_MCP_COMPAT_BIN=/path/to/older/hbots bun scripts/test-mcp-sdk.mjs
```

The baseline must lack the management credential API; the test verifies that
before treating it as a pre-MCP compatibility check. Dedicated
MCP compatibility cases cover old homes defaulting off, partial settings writes,
legacy unscoped credentials, and simultaneous gateways sharing credential files.
Credential read/modify/write operations use OS locks and atomic mode-0600 files.

## Settings design

The MCP section in Access settings extends hbots' existing narrow, left-aligned settings
form and semantic theme roles. It uses the existing type scale, switch geometry,
44-point targets and grouped controls. The endpoint and one-time credential are
the functional focal points. Token creation is disclosed on demand, permissions
use descriptive rows, and a separate result shows Copy and Done. Existing tokens
remain visible for revocation even while MCP is disabled; no new decorative palette or marketing layout is
introduced. English and French strings use the normal locale tables.
