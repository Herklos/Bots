# Management MCP acceptance evidence

The feature follows the reviewed [plan](management-mcp-plan.md), with the user's
subsequent UI instruction: MCP belongs at the end of **Access**, with no separate
Integrations tab.

| Requirement | Implementation and verification |
| --- | --- |
| API parity and automatic generation | `api/router/mod.rs` declares 102 application operations. Its registration macro generates operation metadata from the HTTP method/handler chain. Infrastructure routes use the same approach in `router/infrastructure.rs`. The frozen coverage artifact classifies all 115 entries; mutation contracts are mandatory. |
| Bots and groups | Shared HTTP handlers execute creation, updates, membership and deletion. SDK fixtures exercise real filesystem persistence; unit flows verify model/runtime caps and provisioning behavior through the shared handler. |
| Conversations, routines, approvals, voice | All existing routes are tools. Recorded mock-harness flows verify outgoing messages and audio attachments, pending-request answers and owned routine create/list/enable/run/delete. |
| Remaining API domains | Library, connectors, memory, computer, Vault, providers, usage, settings, storage, access and system operations are in the generated catalog. They call the original Axum application handlers in-process. Existing domain/adapter tests remain applicable. |
| JSON and binary contracts | Canonical request schemas generate tools and OpenAPI; `jsonschema` validates arguments and decoded JSON bodies. Binary operations preserve MIME type, original bytes and disposition, including JSON files. Images also produce native MCP image content. Explicitly extensible fields remain dynamic. |
| Default-off setting and revocation | Missing/invalid persisted flags normalize to off. Validation rejects malformed patches; partial patches preserve the current flag. API and SDK tests verify enable/disable without restart and that old connections stop working. Already-started operations may finish. |
| Credential boundary and scopes | Dedicated hashed bearer tokens, read-only issuance default, independent read/write/secrets/computer/admin scopes, discovery filtering and execution checks. Credential control plane is excluded from tools. Only self-disable is allowed through MCP. Legacy unscoped records retain their original access. |
| Shared homes and sensitive storage | Atomic mode-0600 credential writes, per-process mutex plus OS lock, corruption preservation, creation/last-used metadata and separate Bot tokens. Two-process SDK testing verifies revocation cannot be undone by last-used writes. |
| Network boundary | Peer, Host, Origin and proxy-header checks; explicit rejection of the Tailscale listener even when its socket peer is loopback. LAN/tunnel configuration does not enable remote management MCP. |
| Protocol behavior | Supported released versions are negotiated explicitly. JSON-RPC structure/unknown-tool errors are distinct from typed tool execution errors. JSON responses, no-store headers, notifications, size/concurrency/time limits, and native images are covered by Rust and official-SDK tests. No unimplemented capabilities are advertised. |
| Settings UX and localization | Final section in Access, named token form, read-only default, explained permission rows, separate one-time Copy/Done result and revocation while disabled. English/French tables, browser flow, Contract checks and macOS build. The server supplies the configured local endpoint rather than a fixed port. |
| Documentation and CI | Connection guide, generated tools/OpenAPI/coverage, repeatable generation/UI/SDK commands, pinned SDK fixture lockfile, and SDK smoke test in both Rust CI OS jobs. Normal tests enforce artifact freshness. |

The legacy compatibility script still has four unrelated failures for its removed
Projects REST route; its remaining 26 checks passed. The dedicated SDK test also
passed with a pre-MCP **hbots 0.3.0** binary (`target/release/hbots`, SHA-256
`1e0b4568e3576622191f3917208d605c310872c8c762490b3c0222d9b4808cc5`),
verifying shared sessions and that older settings writers preserve the MCP flag.
All SDK processes use temporary homes. Live-account T3/Anytype testing was not
performed. The macOS result is a successful
build, not a manual native-app interaction test.
