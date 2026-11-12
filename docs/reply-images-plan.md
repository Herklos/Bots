# Plan: reply images, then small UX items

| Field | Value |
|---|---|
| Status | Draft for review. Nothing here is implemented |
| Date | 2026-11-12 |
| Related | `docs/memory-store-plan.md` (Phase 1 adds the Workspace methods this plan needs), `docs/redesign.md` (ImageCard), `docs/webhook-triggers-plan.md` |

Facts are marked **[verified]** (read in this tree) or **[unverified]** (needs a Phase 0 check).

## 1. Summary

A Bot cannot show the person an image it made. It can write a PNG into its folder and mention the name, and the Library shelf will list it, but the chat shows only text. The client already renders images, audio and file cards on a message (`app/chat/message.contract` lines 200 to 215), and the gateway already serves files from `library/` (`api/library_api.rs::library_file`). The missing piece is the link between a Bot's reply text and a file in its folder.

Plan: the Bot saves an image under `library/` and writes a Markdown image reference. The gateway recognizes that reference in the reply, checks the file, and adds it to the message's `files`. The client then shows it with the code it already has. A small MCP tool gives the Bot immediate feedback when a file is too big or not an image. After that come the small UX items that make images pleasant to use: a viewer, save and copy actions, and layout that does not jump.

## 2. What exists today

**[verified]** unless marked.

- `chat/t3.rs::project_turns` builds each message with `files: files_of(&attachments)` where attachments come from the T3 item. `files_of` returns `{ id, name, mime, bytes, kind }` with `kind` `image`, `audio` or `file`.
- The client builds each file's URL in `app/host/session.ts:96-98` as `/api/bots/{slug}/attachments/{id}`. That route reads the gateway's own store, `bots/<slug>/.bot/attachments/<uuid>.bin` (`chat/attachments.rs`). An id that is not a gateway UUID is a 404. So an attachment that T3 attached to an assistant message would not display. **[unverified]** whether any provider attaches files to assistant messages at all (check I0.2).
- `FileRef` in `app/ui/shapes.contract` is `{ id, name, kind, size, url }`. The message view renders an image as `link href=f.url target="_blank"` around an `image` (max width 20 rem).
- `library_file` serves `library/*` through `Library::file_bytes` with a content type chosen by extension in `library_mime`: only html, png, jpg and jpeg. A gif or webp is sent as `application/octet-stream`. No `X-Content-Type-Options`, no size cap, no check that the bytes are an image.
- `LocalDisk::read` follows symlinks. `file_bytes` rejects `..` and requires the `library/` prefix, but a symlink inside `library/` pointing outside the Bot folder would be followed. `docs/memory-store-plan.md` Phase 1 adds `Workspace::symlink_kind` to fix this class of bug.
- `bots/library/kinds.rs::is_picture` already counts png, jpg, jpeg, gif and webp as pictures for the shelf, so images saved in `library/` appear in the panel's shelf today.
- `chat_view` (`api/chat_api.rs`) is polled through `hold_until_change` every 500 ms and tagged by `transcript_tag`, a hash of `messages`, `events`, `requests`, `working`, `pins`. Anything added to `messages` changes the tag. File IO inside `chat_view` therefore must not run on every poll.
- `thread_meta/preview.rs` previews a message with attachments as "Attachment". A reply that is only an image reference, once its syntax is stripped, would have empty text and an empty preview.
- Exact's `text ... markup="markdown"` is used for bot text. **[unverified]** what it does with `![alt](url)` (renders, ignores, or loads a remote URL). This matters for privacy (I0.1).
- The Computer overlay (`app/computer/overlay.contract`) shows JPEG frames streamed from the Bot's browser. It is a remote view, not an embedded web view.
- Limits to respect: Rust files at most 500 lines, app contract files at most 300.

## 3. Goals and non-goals

Goals

1. A Bot reply can show images that live in its own `library/` folder, inline in the chat, in 1:1 chats first, then groups and child threads.
2. Hard limits: PNG, JPEG, GIF, WebP only, 5 MiB each, at most 8 per message, verified by file signature and not just extension.
3. No outbound requests from the client to third-party image hosts, and no served file that is not an image.
4. Clear feedback to the Bot when a reference is rejected.
5. Small UX items listed in section 7.

Non-goals

- SVG (script risk), HEIC, AVIF, video. Audio keeps its existing path.
- Remote images in replies. A `http(s)` image reference is shown as a link, never fetched.
- Generating images. The Bot already has tools for that. This only displays the result.
- A built-in web view. See U4.

## 4. Phase 0: verification spikes

| # | Question | How | Decides |
|---|---|---|---|
| I0.1 | What does the Exact markdown renderer do with `![alt](https://example.com/x.png)` and `![alt](library/x.png)`? Does it fetch remote URLs? | Add a fixture message with both, run `app/host/fixture.ts` in the web build, watch the network panel | Whether the gateway must strip **all** image syntax from text (it must if the renderer fetches). Privacy gate: remote fetch from a Bot reply leaks the person's IP and lets a prompt-injected Bot exfiltrate data in a query string |
| I0.2 | Does T3 ever put attachments on assistant messages? What are their ids, and does T3 serve their bytes at an HTTP path? | Read `tests/fixtures/t3`, ask a Claude Bot for a chart, read the projection | Whether a second source of images exists and needs the same serving path |
| I0.3 | Where do Bots write pictures today, and what text do they use to refer to them? | Read recent Bot folders and transcripts, including browser `take` screenshots | The accepted reference forms and whether `library/` only is too strict |
| I0.4 | Is `Workspace::symlink_kind` available yet? | Check the state of memory Phase 1 | Whether this plan includes that PR |
| I0.5 | Does the image element expose a load error, or a way to reserve space from known width and height? | Try in the web and macOS builds | Fallback UI and the `width`, `height` fields (5.5) |
| I0.6 | Does `link target="_blank"` open inside the app or the system browser on macOS and iOS hosts? | Try the native builds (`docs/native.md`) | Priority of the viewer (U1) |
| I0.7 | In a group room, where does a message's author come from (`from` slug), and can the gateway map it to a folder? | Read `api/rooms_api.rs` and `chat/rooms/*` | Phase 4 scope |

Gate: I0.1 and I0.4.

## 5. Design

### 5.1 Reference forms accepted

A reply text may contain `![alt](target)`. A `target` is accepted when, after trimming and removing one pair of `<...>` or quotes, it:

- is a relative path starting with `library/` or `./library/`, or
- is an absolute path that starts with the Bot's own folder (`bot.workspaceRoot`, already on every roster entry) and then `library/`.

It is normalized by percent-decoding once, then rejected when it contains `..`, a backslash, NUL or another control character, an empty segment, a segment starting with `.`, a scheme (`http:`, `https:`, `data:`, `file:`, `javascript:`), or a leading `//`. The extension must be `png`, `jpg`, `jpeg`, `gif` or `webp`, case-insensitive.

References inside fenced code blocks and inline code are ignored, so a Bot explaining the syntax does not render an image. At most 8 references per message count, duplicates by normalized path are merged.

Rejected references are not an error to the person. The syntax is replaced by its alt text in `[brackets]` or removed, so the reply still reads naturally.

### 5.2 Pure extraction, lazy validation

Two steps, so polling stays cheap.

1. **Extract (pure, no IO).** `chat/images.rs::refs(text, workspace_root) -> Vec<ImageRef>` and `chat/images.rs::strip(text, &resolved) -> String`. Used by `project_turns` callers, by `thread_meta/preview.rs` (an image-only reply previews as "Image"), and by the Library.
2. **Validate (async, cached).** `chat/images.rs::Resolver` holds a bounded cache (256 entries, 10 minute life) keyed by `(slug, path)`. On first sight it:
   - asks `Workspace::symlink_kind(rel)`, and requires `File` (not a symlink, not a directory),
   - reads at most 5 MiB + 1 bytes and rejects larger,
   - checks the signature against the extension: PNG `89 50 4E 47 0D 0A 1A 0A`, JPEG `FF D8 FF`, GIF `GIF87a` or `GIF89a`, WebP `RIFF....WEBP`,
   - reads width and height from the header (PNG IHDR, GIF logical screen, JPEG SOF scan, WebP VP8, VP8L or VP8X) for layout (5.5),
   - stores `{ mime, bytes, width, height, etag }` or a typed failure.

   A failure is cached for 5 seconds so a file written just after the reply appears on the next poll, and is final once the message is older than 30 seconds.

`chat_view` and the group read call the resolver after `project_turns`, once per message that has references and is not cached. The hit path is a map lookup, so the 500 ms poll does no file IO.

### 5.3 Message shape

For each resolved reference the message gains a `files` entry, appended after any T3 attachments:

```json
{ "id": "lib:1f3a9c2e", "name": "chart.png", "mime": "image/png", "bytes": 48210,
  "kind": "image", "path": "library/chart.png", "width": 1200, "height": 800 }
```

`id` is the first 8 hex digits of sha256 of the normalized path. The message `text` has the resolved syntax removed. `transcript_tag` needs no change, because `messages` is already hashed. A failed validation stays as alt text.

The client URL for such a file is built in `app/host/session.ts` from `path`:
`${API}/api/bots/{slug}/library/file?path={encodeURIComponent(path)}`. `FileRef` in `shapes.contract` gains `width: number` and `height: number` (0 when unknown). Nothing else in the contract changes, so the existing `for f in row.files` block renders it.

### 5.4 Serving hardening (`api/library_api.rs`)

`library_file` changes, for every file it serves:

| Change | Reason |
|---|---|
| `Workspace::symlink_kind` must be `File` | A symlink in `library/` must not leak files outside the Bot folder |
| Size cap 5 MiB for images, 413 beyond | Bound memory and bandwidth |
| For image extensions, verify the signature, 415 on mismatch | A renamed non-image is never served as an image |
| Content types for gif and webp added in `library_mime` | They are octet-stream today |
| `X-Content-Type-Options: nosniff` always | Match the attachment route |
| Images: `Content-Disposition: inline`. Downloads (`?download=1`): `attachment` | Viewer and Save need both |
| `ETag` (sha256 prefix of the bytes), `Cache-Control: private, no-cache`, honor `If-None-Match` with 304 | A chart regenerated under the same name must refresh, and unchanged ones should not resend |

SVG stays unserved as an image type. The route keeps serving `html` pages as it does today.

### 5.5 Layout stability

An image that loads after the row renders changes the row height and can break "stay at the bottom" behavior (`app/scroll/model.contract`). The resolver gives `width` and `height`, and the message view sets the image box from them (`aspect-ratio`, `max-width 20rem`) before bytes arrive. With unknown dimensions (0) the old behavior stays.

### 5.6 Giving the Bot feedback: `image_show`

A silent failure ("the image never appeared") is bad for a model that cannot see the chat. Add one MCP tool in `bots/image_tools.rs`, registered in `platform/mcp.rs`, identity from `threadId` the same way `memory_tools` does it:

`image_show { threadId, path, alt? } -> { markdown, bytes, mime, width, height }`

- `path` may be anywhere inside the Bot's folder that is not a hidden directory or `memory/`. If it is outside `library/`, the tool copies it to `library/` with a collision-safe name through `Workspace`, and returns the reference to the copy. Screenshots written to a scratch folder then work without the Bot knowing the rule.
- It runs the same validation as 5.2 and returns a clear error for each failure: "not an image", "larger than 5 MiB", "unsupported type", "is a symlink".
- It returns the exact Markdown to paste, so the Bot does not have to guess the syntax.

The AGENTS.md template (`bots/instructions.rs`, Memory and Library part) gains one line: "To show the person a picture, save it as PNG, JPEG, GIF or WebP under `library/`, call `image_show`, and put the Markdown it returns in your reply." The startup refresh already rewrites every Bot's managed block, so existing Bots get it.

### 5.7 Security summary

| Threat | Mitigation |
|---|---|
| Client fetches a remote image from a reply (tracking, exfiltration in a URL) | Only `library/` references resolve. All other image syntax is stripped from the text before the client sees it (and I0.1 confirms the renderer never fetches) |
| Symlink in `library/` reads files outside the Bot folder | `symlink_kind` refusal at validation and at serve time |
| A non-image served with an image type | Signature check on both paths, `nosniff` |
| Big or many images exhaust memory or bandwidth | 5 MiB, 8 per message, bounded cache, no IO on cache hits |
| Path tricks (`..`, encoded dots, hidden dirs, absolute paths outside the folder) | Normalization in 5.1, then `Workspace` `safe_rel`, then `symlink_kind` |
| A reply tries to show another Bot's file | The path is resolved only under the **author** Bot's folder |
| SVG scripts | Not supported |

Noticed while reading, not part of this work: `library_file` serves a Bot-written `.html` page with `text/html` from the app's own origin, so script in that page runs with the person's session. A `Content-Security-Policy: sandbox` header on html responses would close it. Raise it separately.

## 6. Phases for reply images

Sizes: S under half a day, M about a day, L several days.

### Phase 1: gateway extraction, validation, serving , L

Files: `chat/images.rs` (new, split into `images/refs.rs`, `images/sniff.rs`, `images/resolver.rs` if it passes 500 lines), `api/library_api.rs`, `bots/library/mod.rs` (`file_bytes` returns a typed error), `api/chat_api.rs` (call the resolver), `chat/thread_meta/preview.rs`, `storage/workspace.rs` and adapters (only if memory Phase 1 has not landed: add `symlink_kind` there, shared).

Tasks

1. `refs` and `strip` with the forms in 5.1. Unit tests for every accepted and rejected form, code fences, inline code, percent-encoding, `<path with spaces>`, 9 references.
2. Signature sniffers and dimension readers with tiny fixture images (a few bytes each) for all four formats, plus truncated and lying files.
3. `Resolver` with the cache and the failure timings. Tests with a virtual clock: written-late file appears, deleted file turns into alt text, a changed file gets a new etag.
4. `library_file` hardening (5.4). Tests: symlink, 5 MiB + 1, renamed text file, gif and webp types, 304, download disposition.
5. Wire into `chat_view`. Check `transcript_tag` stability across polls with no change.
6. Preview text: image-only reply gives "Image".

Exit: a fixture thread with a reply `![chart](library/chart.png)` produces a message with one image `files` entry and clean text, and `curl` of the file returns the PNG with the right headers.

### Phase 2: client , M

Files: `app/host/session.ts`, `app/ui/shapes.contract`, `app/chat/message.contract`, `app/host/fixture.ts`, `app/strings/en.json`, `fr.json`.

Tasks

1. URL from `path` in `session.ts`. Keep the attachment URL for ids without `path`.
2. `FileRef.width` and `height`, and reserve the box in the message view (5.5).
3. A fixture row with a library image and one with a broken one, for the e2e and the visual check.
4. Alt text: `aria-label=f.name` is already there. Use the Markdown alt when present as `name`.
5. `scripts/check-contract.mjs` and size checks green.

Exit: open the fixture in the web build, image shows inline with no layout jump, no network request leaves the app origin.

### Phase 3: Bot-facing tool and prompt , M

Files: `bots/image_tools.rs`, `platform/mcp.rs`, `bots/instructions.rs`.

Tasks: `image_show` (5.6), registration in `tools()` and `call_tool`, template line, tests (copy from a scratch folder, symlink refused, too big, wrong type, subagent writes its own `library/` only). Manual test with a real Bot: ask it to make a chart and show it.

Exit: a Bot, asked to draw a chart, calls `image_show` and the chart appears in chat.

### Phase 4: groups, child threads, routines, shelf , M

Tasks (scope set by I0.7)

1. Group rooms: resolve each message against its **author's** folder (`from` slug). A member's image shows in the room.
2. Child threads: durable subagents have their own folder and resolve normally. Ephemeral T3 child threads run elsewhere, so their references do not resolve. Document it.
3. Routine and webhook-born turns: the same resolver applies. Add a test with a `scheduledTaskId` message that references an image.
4. Shelf: confirm images from replies show in Pictures. Extend `is_picture` only if I0.3 finds other extensions in use.
5. Notifications: the preview for an image-only reply says "Image".

Exit: a group room shows an image from one member, a routine's image appears in the 1:1 chat.

### Phase 5: hardening and release , S

Docs (`docs/architecture.md`, README feature line, CHANGELOG), a manual run on web, macOS and iOS builds, a memory and bandwidth check with 8 large images, e2e coverage via `tests/e2e.sh`.

## 7. Small UX items

Ordered by value for the effort. Each one is independent after Phase 2.

### U1 Image viewer (M)

Why: `target="_blank"` leaves the app on some hosts (I0.6) and a 20 rem thumbnail is small for a chart.

How: a viewer overlay as a route of the root view, the same pattern as `ComputerOverlay` (`app/computer/overlay.contract`: values and actions injected). New `app/chat/viewer.contract` and state in `app/chat/model.contract`: `viewerRow`, `viewerIndex`. Features: close on Esc, click outside or a close button, previous and next across all images in the open chat (arrow keys), name and size, Download, Copy where the host allows image clipboard, focus trap and return focus to the thumbnail, `aria-modal`. No zoom in v1.

Tests: contract check, e2e opens and closes it, keyboard navigation, focus return.

### U2 Save and copy image in the message menu (S)

The message menu (`message.contract`, reply, pin, copy) gains "Save image" (download via `?download=1`) and "Copy image" for rows that have an image file. Strings in en and fr.

### U3 Split view (L, spike first, optional)

Goal: two chats side by side on wide screens, `Cmd+\` to toggle (Nuphos uses `Cmd+D`, but a browser reserves it for bookmarks).

Spike (S): measure how much of the root state is single-slug (`slug` in the root model, drafts keyed per Bot already, `holds` keyed per `chat\n{slug}\n{child}` so two different Bots can hold at once, but the same Bot twice would supersede the first read). Build only if the spike shows the change fits under the 300-line contract rule without a big refactor. Rules if built: the same Bot cannot be in both panes, each pane keeps its own scroll and draft, the focused pane receives keyboard input, only shown at 1100 px wide or more.

### U4 Built-in browser tab (skip, decision needed)

The repo's Computer view is a streamed picture of the Bot's own browser, not an embedded web view, so a "browser tab" would be a new kind of surface that only exists on native hosts. Recommend not building it. Offer a cheaper item instead: an **Open in Computer** action on link chips that asks the Bot to open that URL in its browser, using the existing `computer_open` and steer path.

### U5 Smaller polish that images expose (S each)

- Clear error text when an attachment is over 8 MiB (the gateway already refuses; check that the client message is readable).
- Do not scroll away from the reader's position when an image above the viewport finishes loading (covered by 5.5).
- Alt text from the Markdown shown to screen readers (Phase 2, item 4).

## 8. Test summary

| Layer | What |
|---|---|
| Unit | reference parsing, sniffers, dimension readers, resolver cache timing |
| Router | `library_file` headers, 413, 415, 304, symlink refusal |
| Gateway | `chat_view` adds files, tag stable, preview "Image" |
| Contract | `check-contract.mjs`, size check, en and fr strings |
| e2e | fixture chat with image, viewer open and close, no cross-origin request |
| Manual | web, macOS, iOS, one real Bot making a chart, a group room, a routine |

## 9. Risks

| Risk | Mitigation |
|---|---|
| The markdown renderer fetches remote images | I0.1 gate. Strip all unresolved image syntax regardless |
| Bots keep using paths outside `library/` | `image_show` copies for them, and the template line teaches the rule |
| Validation reads big files on a hot path | Lazy, cached, 5 MiB cap, no IO on cache hits |
| Image dimension parsing bugs on odd files | Fall back to 0 and old layout, fuzz the readers with truncated inputs |
| Overlap with memory Phase 1 on `Workspace` methods | Whoever lands first owns the PR. The other rebases |
| Scope creep into a gallery or editor | Out of scope. The Shelf already lists pictures |

## 10. Decisions needed from the owner

1. **`library/` only, or the whole Bot folder?** Recommend `library/` plus the `image_show` copy step. It matches the Shelf and keeps the allow-list small.
2. **Split view:** run the spike, or drop it. Recommend spike after Phase 3.
3. **Built-in browser tab:** skip, and add "Open in Computer" instead (U4). Recommend yes.
4. **Treat the html CSP issue** as a separate small fix now? Recommend yes, it is one header.

## 11. Phase 0 results

To be filled in during I0.1 to I0.7.
