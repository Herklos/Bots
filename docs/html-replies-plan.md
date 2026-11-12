# Plan: HTML visual replies and MCP Apps

Let a Bot answer with a page (chart, table, diagram, mockup) shown inline in the chat, and later let MCP servers a Bot uses return interactive apps. Both come from T3 and work with every provider that runs under it. hbots must project them, serve them safely and draw them.

Status: plan. Nothing here is built. Facts are tagged **[verified]** (read in upcomputer's source, a T3 fork, or in this repo) or **[to verify]** (Phase 0 spike).

Related: `docs/reply-images-plan.md` (images from markdown) shares the serving hardening and the `files` shape. The two ship independently.

## 1. Facts

### 1.1 The T3 side **[verified in upcomputer source]**

- Two MCP tools in `apps/server/src/mcp/toolkits/html/tools.ts`:
  - `html_preview({ html, width?, appearance? })`: renders in a headless browser and returns a PNG, `contentHeight`, console messages and missing images. The browser (about 120 MB) downloads on first use. Pages publish without it.
  - `html_render({ html, title, height })`: publishes the page. `html` is at most 512,000 characters, `title` at most 200, `height` 80 to 2000. Success is `{ htmlRender: { attachmentId, title, height, heights? }, message }`.
- Publish rules (`HtmlRender.ts`): local images by absolute path are inlined (10 MiB each), the final page is at most 25 MiB, and `vh` units are rejected because the frame fits the page height.
- The page is stored as a server attachment `<attachmentId>.html` (the `-html` suffix makes the asset route serve it as a sandboxed document) and is deleted with its thread.
- Clients get bytes through RPC `assets.createUrl` with `{ resource: { _tag: "attachment", attachmentId, fileName, mimeType: "text/html", disposition: "inline" } }`, which returns `{ relativeUrl, expiresAt }`. The relative URL is signed and expires; the web client keeps the first URL and re-mints when under 5 minutes remain.
- The client frame (`HtmlRenderFrame.tsx`) is sandboxed, fits its height to the live page (up to 20,000 px) using the measured `heights` per width as a fallback, injects the theme (`htmlRenderThemeFragment`, `injectHtmlRenderBootstrap` in `packages/shared/src/htmlRender.ts`), opens links in the system browser and offers full screen.
- MCP Apps (`packages/shared/src/mcpApp.ts`, `apps/server/src/mcpApps/*`): a tool result carries the output key `t3McpApp` with `{ attachmentId, server, tool, resourceUri, csp?, permissions?, prefersBorder? }`. The `ui://` resource is snapshotted as an HTML attachment (5 MiB cap) with a CSP taken from `_meta.ui.csp`. App requests use RPC `mcpApps.callTool`, `readResource`, `toolInfo`, `updateModelContext`, each keyed by `{ threadId, itemId }`. Errors: `not-an-app`, `provider-unsupported`, `session-stopped`, `tool-not-callable`, `unsupported-content`, `request-failed`. Only Codex and Claude support it (Claude Code 2.1.301 or newer). The app needs its thread's session running.
- Approvals in upcomputer: an app may call tools on its own server, asking first unless the tool is read-only, and it asks before every message post.

### 1.2 The hbots side **[verified]**

- No code in `gateway/src` or `app/` reads `htmlRender`, `t3McpApp` or `mcpApps`. The transcript drops tool items (`project.rs`).
- The Library already classifies `.html` as kind `page` (`bots/library/kinds.rs`).
- `hbots_design::tokens()` provides the palette, so page theming can follow the app.
- Per-Bot MCP connectors exist (README), so MCP Apps have a source.
- The gateway holds the T3 token; clients never see it. T3 URLs therefore cannot go to the browser. The gateway must fetch and re-serve.

### 1.3 Unknowns **[to verify]**

1. Are `html_render` and `html_preview` offered to hbots Bot threads under the token scopes hbots uses?
2. Which field of the `dynamic_tool` item holds the tool result (`output`, `result`, nested `structuredContent`)?
3. Does Exact have an iframe or webview element on the web build and on native, and can the host receive `postMessage`?
4. Does `assets.createUrl` work for the hbots token, and is the returned `relativeUrl` fetchable by the gateway with that token?
5. Is `html_render` in the stable T3 channel?
6. The exact CSS variable names promised to the model by `HTML_RENDER_THEME_GUIDE` (read `packages/shared/src/htmlRender.ts` past line 140).
7. Whether the session auth is cookie or bearer (decides how an iframe `src` authenticates).

## 2. Goals and non-goals

Goals
- Pages from `html_render` appear above the reply, themed, height-fitted, with full screen, save to library and open in browser.
- Safe by construction: model-written HTML never runs with the app's origin, cookies or token, and cannot reach the network by default.
- MCP Apps: first read-only display, then interactive with the app's tool calls approved in the existing approval cards.

Non-goals
- A page builder or editor.
- Running pages on the gateway origin.
- Pages from providers' own web artefacts. Only T3's tools.

## 3. Design

### 3.1 Projection

New file `gateway/src/harness/adapters/t3/transcript/renders.rs`. For each completed `dynamic_tool` whose `tool_of` is `html_render`, parse the result (`parsed()` handles JSON text) and emit a sibling list:

```json
"renders": [ { "id": "render-<item id>", "ordinal": 41, "at": "...", "kind": "html",
               "title": "Spend by month", "height": 420, "heights": [[320, 900]],
               "attachmentId": "..." } ]
```

Place by ordinal: a render sits above the next assistant message, as T3 does. Running or failed calls produce nothing (a failed `html_render` is a tool error the model retries). `messages` and `events` stay unchanged. Cap 20 renders per transcript page.

For MCP Apps the same module reads `t3McpApp` from any tool item and emits `kind: "mcp-app"` with the reference minus CSP internals the client does not need.

### 3.2 Serving

The client never learns a T3 URL. Two routes, both behind the normal session:

1. `POST /api/threads/{thread}/renders/{attachmentId}/url` returns `{ url, expiresAt }`. It checks that `attachmentId` is in that thread's projected renders (so one Bot cannot read another's pages), then returns a short-lived signed URL `/r/{token}` (HMAC over thread, attachment, expiry, 10 minutes). This mirrors T3's own model and avoids relying on cookies in an iframe load.
2. `GET /r/{token}` verifies the token, calls `assets.createUrl`, fetches the bytes from T3 with the token (cap 25 MiB plus one byte, then fail), injects the theme and bootstrap, and responds with the headers below. A small in-memory LRU (8 MiB total) keyed by `fileRevision` avoids refetching.

Response headers:

```text
Content-Type: text/html; charset=utf-8
Content-Security-Policy: sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline' <libs>;
  style-src 'unsafe-inline' <libs>; font-src data: <libs>; img-src data: blob: <libs>;
  connect-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'
X-Content-Type-Options: nosniff
Referrer-Policy: no-referrer
Cross-Origin-Resource-Policy: same-origin
Cache-Control: private, max-age=300
```

`<libs>` depends on setting `htmlReplies.network`: `none` (empty, self-contained only), `libraries` (default: `cdn.jsdelivr.net cdnjs.cloudflare.com unpkg.com fonts.googleapis.com fonts.gstatic.com`), `open` (`https:`). `sandbox allow-scripts` without `allow-same-origin` gives the page an opaque origin even if opened directly, so it cannot read cookies or call the gateway. `connect-src 'none'` blocks fetch, XHR and WebSocket, which is what stops a page from sending Bot data out. Image and script loads from allowed hosts remain a weak beacon channel, hence the setting.

The client frame adds `sandbox="allow-scripts"`, `referrerpolicy="no-referrer"` and `allow=""`.

### 3.3 Injection

At serve time the gateway inserts, after `<head>`:
- a `<style>` with the CSS variables T3 promises the model, valued from `hbots_design::tokens()` for the requested `theme=light|dark` (names from unknown 6),
- a `<script>` bootstrap that reports `{type:"hbots-height", h}` through a `ResizeObserver`, turns link clicks into `{type:"hbots-open", url}` messages (only `https:` and `mailto:`), and reports `{type:"hbots-error"}` on uncaught errors.
Injection is string-level on a parsed `<head>` position, not a full HTML rewrite; if there is no `<head>` it prepends one. It must be idempotent.

### 3.4 Client (Exact)

- `app/chat/render.contract` (under 300 lines): `HtmlCard` with a title row (title, full screen, save, open in browser), a frame at the fitted height, and loading, failed and "no longer available" states. The height is the reported height when present, else `heights` at the current width, else the agent's `height`, clamped 80 to 4000.
- Full screen reuses the overlay pattern in `app/computer/overlay.contract`.
- Theme switch reloads the frame with the new `theme` parameter (the URL is re-minted if under 5 minutes remain).
- The host must pass `message` events to the contract. Unknown 3 decides whether this is possible on native.

### 3.5 Save to library

`POST .../renders/{id}/save` writes `library/<name>.html` through the `Workspace` trait using the file name rule `htmlRenderFileName` (strip `\ / : * ? " < > | control`, collapse spaces, 120 characters, `.html`). The Library shows it as a `page`. No overwrite: add `-2`, `-3`.

### 3.6 MCP Apps

Read-only first. A gateway endpoint set mirrors the four RPCs, authorised by `{ thread, itemId }` belonging to a Bot the session may read:

| Gateway route | T3 RPC | v1 |
|---|---|---|
| `POST /api/apps/{thread}/{item}/resource` | `mcpApps.readResource` | yes |
| `POST .../tool` | `mcpApps.callTool` | read-only tools only (use `mcpApps.toolInfo.readOnly`) |
| `POST .../context` | `mcpApps.updateModelContext` | v2 |
| message post | `ui/message` | v2, approval card every time |

The frame speaks the MCP Apps JSON-RPC over `postMessage` (`ui/initialize` handshake, protocol version `2026-01-26`). The gateway page that hosts it is the same sandboxed document route with the CSP built from `csp` and `permissions` of the reference, intersected with the network setting (an app can never widen the policy past the setting). `session-stopped` shows "Send a message to wake it".

## 4. Phases

### Phase 0: spikes (S)

- H0.1 Ask a Bot to use `html_render` and `html_preview`. Record whether the tools exist, the item JSON, and where the result lives. Commit redacted fixtures.
- H0.2 Call `assets.createUrl` from a scratch client using the hbots token; fetch the result; record headers and the content type.
- H0.3 Read `HTML_RENDER_THEME_GUIDE` and `HTML_RENDER_LAYOUT_GUIDE`; list the CSS variables.
- H0.4 Exact: iframe or webview support on web and native; `message` events; sandbox attributes; fullscreen overlay pattern.
- H0.5 Session auth type, for the signed URL decision.
- H0.6 A hostile page fixture: tries `parent.document`, `fetch`, `new Image().src`, form submit, `top.location`, `window.open`. Record what the CSP and sandbox stop in a real browser.
- H0.7 Stable-channel check (unknown 5).
- H0.8 One MCP App server (an `ext-apps` example) wired as a Bot connector; capture the `t3McpApp` item.
Exit: unknowns 1 to 7 answered; decisions D1 to D4 taken.

### Phase 1: projection [PR 1], M

- `renders.rs`, wiring into the transcript output, fixtures, unit tests: one render, two renders, a failed render, a `t3McpApp` item, an unknown item, cap of 20.
- Golden test: transcripts without renders are byte-identical.

### Phase 2: serving [PR 2], M

- Token mint and verify, T3 fetch, caps, injection, headers, LRU.
- Tests: attachment not in the thread returns 404; expired token returns 410; oversize returns 413; header snapshot; injection idempotent; no `<head>`; hostile fixture in a browser test if the repo has one (`tests/`).
- Setting `htmlReplies.network` with validation.

### Phase 3: static card in the app [PR 3], L

- `render.contract`, `ChatLog` branch, strings, loading and error states, height handling, light and dark.
- Contract tests for each state with a stub frame.

### Phase 4: polish [PR 4], M

- Full screen, save to library, open in browser, live theme switch, keyboard and screen-reader labels, small screens.

### Phase 5: Bot guidance [PR 5], S

- AGENTS.md template line: Bots may show a chart or table with `html_render`, preview first, do not restate the page in text. Only if H0.1 shows the tools are available. The tool descriptions already instruct the model.

### Phase 6: MCP Apps display [PR 6], M

- Projection for `t3McpApp`, resource route, frame bootstrap, handshake, read-only tool calls.

### Phase 7: MCP Apps interactive [PR 7], L

- `updateModelContext` (latest note per app goes with the next message), `ui/message` through an approval card, non-read-only tools through the existing approval flow, session-stopped state.

### Phase 8: hardening and docs [PR 8], S

- Threat-model note, README features line, CHANGELOG, test on a slow or large page.

## 5. Risks

| Risk | Mitigation |
|---|---|
| Model HTML exfiltrates Bot data | Opaque origin, `connect-src 'none'`, network setting, no cookies reach the frame |
| Exact cannot host an iframe on native | Web first; native shows a "Open in browser" card using the signed URL |
| T3 changes the result shape | Fixtures from two versions; unknown shape yields no card, not an error |
| Large pages slow the gateway | 25 MiB cap, streaming read, LRU, one fetch per token |
| Theme variables drift from T3's guide | Test that every variable named in the guide has a value |
| A Bot tricks a card into another Bot's page | Attachment must be in the same thread's projected renders |

## 6. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | Default `htmlReplies.network` | `libraries` |
| D2 | MCP Apps in the first release | No, after static pages (Phases 6 and 7) |
| D3 | Save to library: button only, or automatic | Button only |
| D4 | Pages on native before Exact supports frames | Show a card that opens the signed URL in the system browser |
