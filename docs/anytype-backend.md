# Anytype as a storage backend: plan

Status: tier A file sync is implemented (M0–M7, 2026-11-12). Choosing Anytype in Settings syncs memory, skills, library files, attachments, and shared memory. Typed stores, the markdown codec, and multi-instance validation are not built. This file is still the design record. The property level type catalog is in [anytype-schema.md](anytype-schema.md). Sources: the gateway code, the Anytype developer docs, the OpenAPI spec served by the local Anytype (`/v2/docs/openapi.json`, API 2025-11-08, pre-release v2), the `anytype-cli` README, and live probes in the `Test-AnyBot` space.

Probe leftovers in `Test-AnyBot`, because the API cannot purge them: one chat (`hbprobe-log`), one discussion and one page (`hbprobe2-doc`) in the Bin, and the deleted types `hbprobe` and `hbprobe_2` with their properties (restorable in the app only).

## Decisions

1. **Anytype first, for sync across several instances on several machines.** Anytype is the authority. Each machine keeps a local working copy because the harness needs a real folder and Anytype may be briefly unreachable.
2. **A dedicated private Anytype space.**
3. **Own `m_` types, as many as needed,** adapted from the person's `anytype-agents` org schema.
4. **The pre-release API (v2) is accepted.** The risk is contained in one module.
5. **Prompts and memory are editable inside Anytype.** The markdown codec is core work.
6. **Storage is configured in Settings.** The default is the file system with a configurable folder. Anytype is added when implemented, with a host (default localhost) and an API key or a pairing code.
7. **One Anytype account and one API key per machine serve every Bot.** Bots are data, not Anytype identities.
8. **No Anytype chat features.** No chat bridge, no chat objects, no discussions, no chat streams.
9. **Multi-instance validation happens after the implementation.** Joining a second Anytype instance, concurrent-merge tests and the headless bot account are phase 9. The design assumes the worst merge behavior so the build does not depend on the result.

## Audit of the previous revisions

Every claim was re-checked against the code, the spec and the probes.

| # | Finding | Severity | Resolution |
|---|---|---|---|
| A1 | "No change feed, poll" was wrong. This build serves `POST /v2/spaces/{id}/search/stream` (SSE: `object_added`, `object_updated`, `object_removed`) | High | One search stream per gateway, polling as fallback |
| A2 | The rate limit applies to writes. The spec lists `429` only on mutating routes and on streams (too many held), not on reads | High | Budget counts writes only. Adaptive limiter driven by real 429s |
| A3 | Delete: `permanent=true` works only for objects the same key created; archiving works for any object the key can write; archived objects vanish from search and refuse edits (409 `object_archived`); objects with a discussion cannot be purged; types and properties are never purged | High | Tombstone property is the delete signal, Bin is garbage collection, no destructive schema operations |
| A4 | Key normalization: requesting `hbprobe2` created `hbprobe_2`, so `m_sha256` would not keep its key | High | No digits in keys, explicit keys, read back created keys into `m_meta` |
| A5 | `Library` already writes `.mcp.json`, `.codex/config.toml`, `.claude/settings.json` through `Workspace`. A deny list in callers would not stop them syncing | High | Path policy enforced inside the synced layer |
| A6 | `Bots::load` rescans the disk and reads every `bot.json` on each call. `bots.list()` has 25 call sites across 7 modules (some in tests) | High | In-memory read-through cache in every typed store |
| A7 | A room keeps per device runtime state (`threads`, `anchors`, `hops`, `lastSeenAt`) and dispatches through the local T3 | High | Runtime state stays local. A group lives on one machine (home machine rule) |
| A8 | Two machines can create the same slug offline | Medium | Identity is `m_bot_id` (uuid). Deterministic collision rule |
| A9 | Joining a second Anytype to the space is not an API operation. Desktop with the same account joins on its own. A headless bot account joins by an invite link created in the app | Medium | Manual step, deferred to phase 9 |
| A10 | Gateways auto-update independently, so versions differ across machines | Medium | `m_schema_version`, `m_min_gateway`, read-only degraded mode |
| A11 | A per device task id map on a shared `m_schedule` makes every machine write one object | Medium | T3 task ids stay local |
| A12 | A human edit in the app changes `last_modified_date`, not our `m_updated` | High | Change detection uses `last_modified_date` and content. Our properties are advisory |
| A13 | Per property last-writer-wins can pair `m_content` from one machine with `m_sha` from another | High | Content is authoritative, sha recomputed |
| A14 | Echo loops, and rewriting a file while a harness turn edits it | High | Manifest sha check. Remote changes to a Bot's folder apply only while it is idle |
| A15 | Limits were guesses. Spec: 1 MiB per string, 1 MiB structured body, 10 MiB document, 128 properties, 2,048 blocks, 512 ops per patch, 256 blocks per op | Medium | 256 KiB inline cap, split larger content |
| A16 | Codec facts: block `text` already carries inline markdown; numbered lists carry no start number; `---` front matter becomes divider plus paragraph; HTML comment becomes a literal paragraph; multi-line quote is one block | Medium | Codec rules and tests |
| A17 | `Test-AnyBot` has a second member (`anybot-live`, editor). Sharing a space exposes everything and lets that member write objects machines pull | Medium | Setup refuses a shared space by default |
| A18 | Views and query views are addressed by minted ids. A type created with an explicit property list drops bundled properties such as `backlinks` | Low | View ids in `m_meta`, bundled properties listed per type |
| A19 | The CLI README says "on v2, a key can only delete objects created under its name" | Medium | Same key name on every machine. Confirmed in phase 9 |
| A20 | A scoped key comes from the Desktop pairing flow (`POST /v2/auth/challenges`, approval in the app, 4 digit code) or `anytype auth apikey create --space ... --read-write`. Scoped keys work on v2 only | Info | Settings offers a Connect button that runs the pairing flow; pasting a key is the advanced path |
| A21 | The storage choice cannot live in the synced settings (chicken and egg), and the folder path is today `HBOTS_ROOT` read in `Config::from_env` before anything else, with `.botapp` inside it | High | Local `~/.hbots/storage.json` plus env override, same locking pattern as network access |
| A22 | `docs/backends.md` says 145 tests. The gateway has about 340 test attributes | Info | Not changed here. Update that doc when P1 lands |
| A23 | `library.json` is one blob with an in-process `Mutex`. Two gateways make that a lost update | Medium | One object per record with compare-and-swap |
| A24 | The `anytype` CLI is installed here (`~/.local/bin/anytype`). A headless instance uses ports 31010 to 31012, the app 31007 to 31009 | Info | Two real instances can run on one machine in phase 9 |

## Verdict

1. **Use the HTTP API (v2), called directly from the gateway. Do not use MCP for storage.** The Anytype MCP is the same API wrapped as tools. It adds a Node subprocess, text-shaped results, local temp-file paths for uploads and downloads, and removes control over retries, rate limiting, streams and headers. Keep MCP for another job: letting a Bot read the person's Anytype as a connector.
2. **Anytype can hold everything the gateway owns.** Out of scope: conversation transcripts and run state (T3's) and secrets (never in Anytype).
3. **It cannot be a pass-through filesystem.** The harness needs a real directory and edits `memory/*.md` with its own file tools. The API has a write limit, no multi-object transactions, per property last-writer-wins merges and a lossy markdown read.
4. **Architecture.** Anytype is the authority, every machine holds a materialized working copy. Tier A: exact files. Tier B: typed objects. Tier C: page-backed documents with a markdown codec. Writes apply locally first, then push.
5. **Transport between machines is Anytype's own sync.** Each machine runs its own Anytype, joined to the same space. A gateway talks only to the Anytype on its own machine.
6. **One account, one key per machine, for every Bot.**
7. **Multi-machine safety rests on three things:** each Bot has a home machine, append-only streams for anything many machines write, and content-authoritative sync with a local history ring.
8. **The `Workspace` trait needs changes first,** plus a path policy layer, typed store traits and a storage settings model.

## What exists today

Everything is files under `~/.hbots` (`HBOTS_HOME`). The Bots live in `HBOTS_ROOT`, default `~/.hbots/bots`, with app state in `bots/.botapp`. One `Workspace` trait exists (`gateway/src/workspace.rs`, `LocalDisk` plus a test `MemoryWorkspace`). Only `library.rs` uses it (library files, attachment listing, records, connectors and the `.mcp.json`, `.codex/config.toml`, `.claude/settings.json` merges). The rest read and write paths directly.

| Data | Where today | Written by | Harness reads | Goes to Anytype |
|---|---|---|---|---|
| Bot state and config | `<bot>/.bot/bot.json` | gateway | no | shared fields (`m_bot`) |
| T3 binding (`projectId`, `mainThreadId`), `lastSeenAt`, computer override, routine to T3 task ids | `bot.json`, T3 | gateway | no | **no**, per machine (`.bot/local.json`) |
| Prompt and rendered instructions | `AGENTS.md`, `CLAUDE.md` symlink | gateway | yes | prompt yes (`m_prompt`), rendered file no |
| Tool wiring | `.mcp.json`, `.codex/config.toml`, `.claude/settings.json` | gateway | yes | **no**, holds the Bot bearer token |
| Memory | `memory/MEMORY.md`, `memory/log.md`, notes, `.botapp/shared-memory.md` | harness, gateway tools | yes | yes |
| Skills, library files | `skills/`, `library/` | person, harness | yes | yes |
| Library records | one blob `.bot/library.json` | gateway via `Workspace` | no | one object per record |
| Connectors | `.bot/connectors.json` | gateway | indirectly | yes without secrets |
| Attachments | `.bot/attachments/<uuid>.json` and `.bin` | gateway | no | yes |
| Groups | `.botapp/groups.json`: shared fields, `messages`, runtime state | gateway | no | shared fields and messages yes, runtime state no |
| Settings | `.botapp/settings.json`, `main-bot.json`, `apps.json` | gateway | no | shared prefs yes (`userName`, `autoReview`, `rules`, `sections`, `defaultModel`, `avatarDrawingModel`, `defaultRuntimeMode`, `groupAnswerPolicy`, `beta`), UI prefs no (`theme`, `palette`, `accent`, `timezone`) |
| Reactions and pins, usage, routine history | `.botapp/thread-meta.json`, `usage.json`, `routine-history.json` | gateway | no | local |
| Routines | T3 `scheduledTasks` | T3 | no | definitions yes (`m_schedule`), executor stays T3 |
| Secrets | `vault.enc`, `.vault-key`, `access.json`, `session-secret`, `.bot-mcp-token`, `t3-token`, `.env` | gateway | no | **never** |
| Storage backend config (new) | `~/.hbots/storage.json`, `~/.hbots/anytype-key` | gateway | no | **never** |
| Transcripts, run state | T3's database | T3 | n/a | out of scope |

## Data model at a glance

Property level detail is in [anytype-schema.md](anytype-schema.md). All keys start with `m_`.

| Group | Types | Tier | Phase |
|---|---|---|---|
| Core | `m_meta`, `m_device`, `m_settings`, `m_bot` (profile layout), `m_file`, `m_conflict` | A and B | P2 to P4 |
| Records | `m_project`, `m_note`, `m_task` (todo layout), `m_plan`, `m_link` (bookmark layout), `m_repo`, `m_machine`, `m_service`, `m_attachment` | B | P4 |
| Conversation | `m_group`, `m_msg_segment` | B | P4 |
| Routines and models | `m_schedule`, `m_model_profile`, `m_connector` | B | P4 |
| Documents | `m_prompt`, `m_memory`, `m_skill`, `m_log` | C | P5 |
| Catalog | `m_team`, `m_subagent`, `m_harness`, `m_computer`, `m_tool_profile`, `m_vault_grant`, `m_suggestion`, `m_approval`, `m_usage_day` | B | P7 |

Each type ships with a default template, a default view, an icon and a generated query. Statuses and kinds that Michel owns are select properties, everything that follows an outside system is text.

## API or MCP

| | HTTP API v2 | Anytype MCP |
|---|---|---|
| Surface | the source (42 paths) | same operations generated from the OpenAPI spec |
| Runs as | `reqwest` in the gateway (already a dependency, with `sha2`, `async-trait`, `tokio-stream`) | an `npx` Node process holding the key in an env var |
| Concurrency | `If-Match`/`ETag`, `Idempotency-Key` on every mutation, `?dry_run=true` | `expected_etag`, `request_key`, `dry_run` on some tools |
| Streams | SSE search stream | none |
| Files | multipart upload, ranged and conditional download | upload takes a local path, download saves to a temp path |
| Rate limiting, retries | ours | invisible to the caller |
| Fit | a daemon storing data | an LLM working with the person's notes |

The person's `anytype-agents` skill already moved the fleet from MCP to HTTP.

**Pre-release risk (accepted).** v2 "can change without a new API version being minted". Mitigation: one module (`gateway/src/anytype/client.rs`), a small endpoint subset, a contract test, and a startup check that diffs the served `GET /v2/docs/openapi.json` against a checked-in snapshot of the paths and schemas Michel uses. A mismatch gives a readable `api_changed` state and a read-only degraded mode.

## What the probes and the spec established

Live probes on 2026-11-11 against the desktop app (`127.0.0.1:31009`, API `2025-11-08`).

Confirmed by probe:

1. **Text properties are byte exact.**
2. **`format=md` is lossy.** It injects the object name as a `# title`, appends hard-break spaces, escapes `[pref]` as `\[pref]`, turns table cells into `<br>` cells, rewrites `---`, and drops code fence languages. The block JSON (`anyblock`) read is structured and lossless.
3. **Markdown to blocks parse:** headings 1 to 4, bullets and numbered items with `indent`, `checkbox` with `checked`, one `quote` block per quote with `\n` inside, `code` with `language`, `table`, `divider`. A `---` front matter pair becomes divider, paragraph, divider. An HTML comment survives as a literal paragraph. A numbered list carries no start number. A hard break is `\n` inside one paragraph.
4. **Compare-and-swap.** A matching `expected_etag` succeeds. A stale one answers 409 `etag_mismatch` with the current etag. Without it the last write wins. Sync also moves etags.
5. **Binary round trip is exact** (3 MiB, identical hash). The stored name loses its extension and the type is `application/octet-stream`.
6. **Query works:** compact filters, sorts, `fields` projection (only properties the type carries), `type` filter, full-text search.
7. **Bin semantics:** archive removes an object from search and queries and refuses edits until restored. `restore` works for any object the key can write. Archiving another participant's object is allowed, purging is not.
8. **Types and properties are not purged** by delete. Objects keep their values.
9. **Key normalization:** `hbprobe2` became `hbprobe_2`.
10. **Dates** read back as `YYYY-MM-DDT00:00:00Z`. Select values read back as arrays of names. A `last_modified_date` filter returns an object only after it has been edited.
11. **`whoami`:** the key behind the MCP is legacy and unscoped (all 8 spaces). `Test-AnyBot` has two members, the owner and an editor bot account.
12. **Chat messages** (probed, not used by this plan): idempotent with a `request_key`, but the text is re-derived and the author is the key's participant.

From the spec and docs:

- **Auth.** Bearer key. Pairing: `POST /v2/auth/challenges`, the person approves spaces and permissions in Desktop, reads a 4 digit code, then `POST /v2/auth/api_keys`. Neither needs a key. v2 needs no `Anytype-Version` header.
- **Limits.** String 1 MiB, structured body 1 MiB, document body 10 MiB, 128 properties, 2,048 blocks, 512 ops per patch, 256 blocks per op or insert, tables up to 64 columns and 1,024 rows. Lists default to 25 and cap at 1,000.
- **Write rate limit.** 429 only on mutating routes. Documented numbers (v1 guide): sustained 1 per second, burst 60. `ANYTYPE_API_DISABLE_RATE_LIMIT=1` lifts it where the Anytype process starts.
- **Search stream.** `POST /v2/spaces/{id}/search/stream`: body is a search without `query`, `heartbeat` query parameter. Opens with one `object_added` per match and `snapshot_complete`, then `object_added`, `object_updated` (whole requested row, when a requested field, name, type or discussion changes) and `object_removed` (left the set, not necessarily deleted). No ids, no replay, a reconnect is a fresh snapshot. A cap on concurrent streams answers 429.
- **Idempotency.** Mutations accept `Idempotency-Key`: a replay returns the stored answer, the same key with a different body is 409 `idempotency_conflict`.
- **Headless.** `anytype-cli` embeds anytype-heart: bot account (`anytype auth create`), `anytype space join <invite-link>`, scoped keys (`anytype auth apikey create <name> --space <id> --read-write`, v2 only), API on `127.0.0.1:31012`, `--listen-address` and `ANYTYPE_API_ALLOWED_HOSTS`, a launchd or systemd service.

Unknown, covered by the spike or phase 9: the concurrent stream cap, whether `dataview` and `property` blocks, templates and widgets can be authored through the API, how Anytype merges concurrent writes across instances, cross-instance lag, key name purge rights across instances, and whether one headless account can serve several machines. Resolved by the M0 spike (`docs/anytype-spike.md`): the practical size limit is the 10 MiB request body, text properties round-trip byte exact at least to 4 MiB; request latency is single-digit to tens of milliseconds for get, patch, 100-row search and 256 KiB upload; and app body edits do change `last_modified_date`, but it is absent until first modification and only day-precision filters on it match, so poll cursors sort and compare rather than filter.

## Storage settings

A new Settings section, **Storage**, placed with Access. It follows the existing network pattern: a saved choice applies at the next start, an environment variable wins and locks the matching control, and a restart button appears when the process is the installed service.

### What the person sees

```
Storage
  Where Bots are stored    (•) File system   ( ) Anytype        [Anytype shows "Not available yet" until it ships]
  Folder                   [ /Users/me/.hbots/bots            ] [Choose…]
                           Default: ~/.hbots/bots. With Anytype this folder is the local working copy.
                           3 Bots · writable · 180 KB
  ── shown when Anytype is selected ──
  Host                     [ http://127.0.0.1:31009 ]   Default: this computer. Headless Anytype uses port 31012.
  Connection               [ Connect with Anytype… ]    or  Advanced: API key [ •••••••• ] (write only)
                           Connected · key "hbots" · read and write · space "hbots"
  Space                    [ hbots ▾ ]                  (the spaces the key can reach)
  [ Test connection ]      reachable · API 2025-11-08 · key is scoped · write ok · space is private · schema ok
  Sync                     ok · outbox 0 · last change 2 s ago · peers: laptop (lag 4 s)
  [ Save ]                 "Restart required to apply."  [ Restart now ]
```

### API key: does it make sense?

Yes, as the second path. The primary path is **Connect with Anytype**: the gateway starts the pairing flow, the person approves the space and write access in the Anytype app and types the 4 digit code in Settings, and the gateway stores the resulting scoped key. This avoids copy and paste and yields a key limited to one space. The pasted key is for headless Anytype (`anytype auth apikey create`), for a key made ahead of time, and for scripted installs. On a default local desktop install the person never sees a key.

### Fields and rules

| Field | Default | Rules |
|---|---|---|
| Backend | File system | `filesystem` or `anytype`. Anytype is selectable only when the build reports it available |
| Folder | `~/.hbots/bots` (the current `HBOTS_ROOT` default) | Absolute path, `~` expanded. Must exist or be creatable, be writable, not equal the data root `~/.hbots`, not sit inside a Bot folder or `.botapp`. A changed path does not move data. If the old folder holds Bots, Settings offers **Copy Bots here**, which copies, verifies by hash, and never deletes the old folder |
| Host | `http://127.0.0.1:31009` | `http` or `https`, no path, trailing slash stripped. A loopback host is trusted. Any other host shows a warning that the key crosses the network and needs an explicit confirmation |
| API key | none | Write only: the API never returns it, only `keySet`, key name and the grant from `whoami`. Stored in `~/.hbots/anytype-key` with mode 0600. A key that reaches every space is refused unless the person ticks "Allow a key that can reach all my spaces" |
| Space | `hbots` | Chosen from the spaces the key can reach, or created in the app. A space with other members is refused unless overridden |
| Key name | `hbots` | Advanced and read only. Same on every machine (see A19) |

### Persistence, precedence, locking

- **File:** `~/.hbots/storage.json`, written atomically, outside the Bots root so a folder change cannot move its own config.

```json
{ "backend": "filesystem", "root": null,
  "anytype": { "host": "http://127.0.0.1:31009", "space": "hbots", "keyName": "hbots" } }
```

- **Key:** `~/.hbots/anytype-key`, 0600, permissions checked at start.
- **Precedence:** environment, then `storage.json`, then defaults. Environment names: `HBOTS_STORAGE`, `HBOTS_ROOT` (existing, the folder), `HBOTS_ANYTYPE_URL`, `HBOTS_ANYTYPE_KEY` or `HBOTS_ANYTYPE_KEY_FILE`, `HBOTS_ANYTYPE_SPACE`. A control whose variable is set is shown locked with the variable named, like `HBOTS_NETWORK_ACCESS` today.
- **Read early:** `Config::from_env` reads `storage.json` in the same step that resolves `HBOTS_ROOT`, before any store is built.

### API (all behind the normal session)

| Route | Purpose |
|---|---|
| `GET /api/storage` | `effective` (what runs), `saved` (what applies next start), `locked` per field, `available` backends, `restartRequired`, `canRestart`, Anytype status. Never the key |
| `PUT /api/storage` | Save backend, folder, host, space. A key goes in a separate write-only field |
| `POST /api/storage/test` | Validate without saving: folder checks, or reachability, version, `whoami`, scope, space privacy, schema state |
| `POST /api/storage/anytype/pair` and `.../pair/confirm` | Start the pairing flow, then exchange the 4 digit code for the key |
| `PUT /api/storage/anytype/key` and `DELETE` | Paste or remove a key |
| `POST /api/storage/copy` | Copy Bots to a new folder, with progress and hash verification |
| `POST /api/storage/restart` | The same restart path as `POST /api/access/network/restart` |
| `POST /api/storage/anytype/import` and `GET .../import` | Dry run and import (P6) |

Changing the backend, host, folder or key is refused for requests that arrive through a tunnel or remote access, as the Bot tool server refuses them, and the key never appears in logs or responses.

### Applying a change

1. Save writes `storage.json` (and the key file). Nothing changes in the running process.
2. Settings shows "Restart required". **Restart now** uses the service restart when the process is the installed service, otherwise it says to restart Michel.
3. On start the gateway resolves the effective configuration. A bad value (missing folder, unreachable Anytype) never prevents start: it falls back to the file system working copy, shows the reason in Settings, and keeps Bots running.
4. First start on Anytype: state `needs_import`. The local tree stays the truth until the person runs the import (dry run first). Nothing is deleted.
5. Switching back to File system only changes the setting. The working copy is intact. **Export** brings it up to date if it is stale.

New chrome strings go in `app/strings/en.json` and `app/strings/fr.json` (`settings.storage` and children). Client tests go in `app/app.test.contract`.

## Architecture

Principles:

1. Anytype is the authority. A new or wiped machine rebuilds from it.
2. Each machine has a working copy for anything the harness touches and applies writes there first.
3. Anytype being down never blocks a Bot. Writes queue and replay.
4. No silent loss of unsynced local edits. A real conflict keeps both versions and tells the person.
5. Secrets and derived files never leave the machine.

```
 handlers, tools ──► typed stores (cached) ──┐
                                             ├──► anytype/client.rs ──► local Anytype ──► Anytype sync ──► other machines
 handlers, tools ──► Workspace ──► PathPolicy ──► SyncedWorkspace ──► LocalDisk (harness cwd)
                                                   │  outbox · stream/poll · manifest · history ring
```

New module `gateway/src/anytype/`:

- `client.rs`: `reqwest` wrapper. Write limiter (default 0.8 per second, burst 40, adapted from real 429s), one `Idempotency-Key` per logical mutation (hash of device, path, sha, operation, so an outbox replay is safe), `If-Match`, pagination walker, SSE reader with reconnect, typed errors (`EtagMismatch`, `RateLimited`, `Unreachable`, `NotGranted`, `ApiChanged`, `ObjectArchived`, `Gone`).
- `schema.rs`: idempotent bootstrap of types, properties, templates, queries and widgets from one declarative table (the schema doc follows the code). Reads actual keys back into `m_meta`. Additive migrations only.
- `pairing.rs`: the pairing flow and `whoami` validation.
- `workspace.rs`: `AnytypeWorkspace`, the remote half of tier A.
- `sync.rs`: manifest, outbox, change detection, apply, conflicts, history ring, tombstones.
- `codec.rs`: markdown to blocks and blocks to markdown.
- `stores.rs`: typed stores with an in-memory read-through cache.
- `canary.rs`: device heartbeat and peer lag.
- `gateway/src/storage_settings.rs`: the settings model, validation and the routes above.

### Path policy (inside the synced layer)

Because `Library` writes `.mcp.json` and friends through `Workspace`, the allow and deny rules live in `PathPolicy`, applied by `SyncedWorkspace` on every call and again on every pulled path.

- Allowed, per Bot: `memory/**`, `skills/**`, `library/**`, `.bot/attachments/**`. Shared: `.botapp/shared-memory.md`.
- Always denied: `.mcp.json`, `.codex/**`, `.claude/**`, `AGENTS.md`, `CLAUDE.md`, `.bot/local.json`, `.bot/connectors.json` (until secrets are split, `m_connector` replaces it), `.botapp/access.json`, `.botapp/session-secret`, `.vault-key`, `vault.enc`, tokens, `.env`, `storage.json`, `anytype-key`.
- Pulled `m_path` values pass `safe_rel`, the allow list and a symlink refusal, and may never target a derived file.

### Tier A: exact files (`m_file`)

One object per synced path, byte for byte. Exact text in `m_content` up to 256 KiB, larger or binary content as an Anytype file in `m_blob`. `m_sha`, `m_size`, `m_mime`, `m_mtime` are ours because Anytype drops extensions and times. `m_deleted` is the tombstone, because an archived object disappears from search and cannot be edited. A tombstone older than 30 days is archived to the Bin by whichever device sees it first. Directories are implicit: `list(rel)` queries `m_dir` and folds deeper paths into entries.

### Tier B: typed objects

Bots, groups, records, routine definitions, settings, devices, connectors, model profiles. Every store keeps an in-memory cache so the 25 `bots.list()` call sites stay as cheap as today. The cache is built from a stream snapshot at start, updated by stream events and by the gateway's own writes, and has a freshness bound so a dead stream cannot serve stale data for long.

### Tier C: page-backed documents (prompts, memory notes, skills)

Edited in Anytype (decision 5), so the canonical content is the object body.

- **Write:** markdown on create, `replace_subtree` or `insert_blocks` on update, at most 256 blocks per operation and 512 operations per patch.
- **Read:** fetch `anyblock` JSON (`ids=full`) and render markdown ourselves.
- **Codec rules:** inline markup passes through (a block's `text` already holds `**`, `*`, `~~`, backticks, links, `<mention>`); `anytype://object?objectId=` links stay; headings 1 to 4; bullets and numbered lists by `indent`, numbering restarted by position; `checkbox` as `- [ ]` and `- [x]`; one `quote` block as a `>` group; `code` with its language; `divider` as `---`; `table` as a pipe table; unknown blocks (callout, toggle, embed, file, dataview) render as a comment line and are kept byte for byte in a sidecar so a round trip does not delete them. Front matter is rejected on write.
- **Normalization is idempotent** (parse, render, parse, render equals one pass). A property based test enforces it. The first write of a file the codec would change is shown as an informational conflict, not rewritten silently.
- **Disk rendering.** The harness reads plain files. `.bot/sync-manifest.json` (local, never synced) maps path to object id, last rendered sha and Anytype `last_modified_date`.
- **Both sides edit.** Base is the last rendered text. A line based three-way merge runs on the rendered markdown. Clean: written to both sides. Otherwise keep both and open an `m_conflict`.
- **Structured memory.** `m_memory` carries category, valid range, provenance and tombstone as properties, matching `docs/memory-maintenance.md`. `@valid(...)` stays in the rendered file.
- **Large documents.** More than 2,048 blocks or 1 MiB of text is split into part objects linked by `m_parent`.

### Change detection

One search stream per gateway on the Michel types, requesting only cheap fields (`m_path`, `m_sha`, `m_deleted`, `m_device`, `m_rev`, `last_modified_date`, `m_slug`), never content. On `object_added` or `object_updated`, fetch the object only if content could have changed: `m_sha` differs from the manifest, or `last_modified_date` is newer than recorded (a human edit in the app). Our own echo is recognized by `m_device` equal to this device and `m_sha` equal to what we pushed.

On reconnect the stream restarts with a fresh snapshot. Diff it against the manifest. An object in the manifest but absent from the snapshot has left the set, possibly archived, so confirm with one read before acting.

Fallback if the stream is refused (stream cap) or missing: a query every 10 seconds on `last_modified_date > cursor OR created_date > cursor`, cursor per device. Reads are free.

### Concurrency model

Anytype merges property by property, last writer wins, and `If-Match` covers only the local view.

1. **Content is authoritative, metadata is advisory.** The decision uses `sha256(m_content)` computed locally. `m_sha`, `m_rev`, `m_device` are hints, so a torn write is detected by recomputing.
2. **Three-way per path.** The manifest keeps `base_sha`. Local changed only: push. Remote changed only: apply. Both: conflict.
3. **Home machine.** Folder files have one writer, the Bot's home machine, plus human edits in the app, handled by the three-way merge.
4. **Append only for fan-in.** Messages, logs and activity are per device segments or one object per entry.
5. **Typed objects merge by property.** Two machines editing different properties of a `m_bot` both survive. Same property: last writer wins, the loser's value stays in the local history ring.
6. **History ring.** Every version this device writes or overwrites is kept (last 20 per path or 14 days) under `.botapp/sync-history/`. It is the safety net for the one race that cannot be removed: a remote overwrite of content this device pushed moments earlier. Detection raises an informational notice.
7. **Idle apply.** A remote change to a Bot's folder is written only while that Bot has no active turn. Writes are atomic (`store::write_atomic`).
8. **Echo guard.** A disk event whose sha equals `base_sha` is ignored.

### Failure handling

| Failure | Behavior |
|---|---|
| Anytype not running at start | Gateway starts and serves the working copy. State `offline`, reconnect with backoff, outbox accumulates (capped, warns at thresholds) |
| 429 on a write | Honor it, shrink the limiter, keep the item in the outbox |
| 409 `etag_mismatch` | Re-read. Remote equals base: retry. Remote equals ours: done. Otherwise conflict. At most three attempts |
| 409 `object_archived` | Restore if our change is newer than the archive, otherwise treat as a delete |
| 410 `object_deleted` | Treat as purged. Recreate only if the local file still exists and is newer |
| Key revoked or space left | State `not_granted`. Reads from the working copy, writes queue, banner in Settings |
| `api_changed` | Read-only degraded mode and a banner |
| Process killed mid-push | Durable idempotent outbox. Before creating an object, search `m_path` to avoid duplicates after a crash |
| Duplicate `m_file` for one path | Keep the earliest `created_date`, merge three-way with the later one, archive the duplicate |
| Folder missing or Anytype unreachable at start | Fall back to the file system working copy and say why in Settings |

### Schema versioning across machines

`m_meta` holds `m_schema_version` and `m_min_gateway`. A gateway older than `m_min_gateway` runs read-only with a banner. A newer gateway runs the additive migration once (guarded by a `m_migrating_by` property) and bumps the version. Readers tolerate unknown properties and options. Writers never send an option the property does not hold.

## Multiple machines

**Identity, plainly.** There is one Anytype account per machine and one API key for everything that machine syncs. Bots are not Anytype accounts. On a desktop that account is the person's own, the same on every machine, and the space appears on each by itself. On a headless machine it is one bot account for all Bots on that machine. Bots, groups and files are just objects in the space.

**Topology.** Each machine runs its own Anytype and a gateway that talks only to it. Anytype's sync moves the data. A machine without Anytype cannot use this backend. Joining a second instance is deferred to phase 9.

**Devices.** A stable device id (`~/.hbots/device-id`) and one `m_device` object per gateway: hostname, version, reachable URLs, stream status, last seen. Writes record the device in `m_device`, not the Anytype participant, because machines may share one account.

**Peer lag.** Every 60 seconds a device updates its own `m_last_seen`. Others read it through the stream and show `now - last_seen` as lag, or "offline". Anytype has no sync status API, so this is the only measurement.

**Bot identity.** `m_bot_id` (uuid) is the identity and the slug is a folder label. If a pull finds two Bots with one slug, the earlier `created_date` keeps it and the later one is renamed `<slug>-2` on every machine, with a notice. Creating a Bot checks the remote slug first when online.

**Where concurrent writes are possible:**

| Data | Writers | Strategy |
|---|---|---|
| Group messages, logs, activity | many machines | append only, per device segments or one object per entry |
| Library records, settings, group metadata | any machine, rarely together | per property merge, content authority, history ring |
| A Bot's folder | its home machine, plus human edits in Anytype | home machine plus three-way merge |
| Bot definition (`m_bot`) | any machine | per property merge |

**Home machine per Bot (the rule behind "one active machine").** This is about which computer runs a Bot, not about Anytype accounts. A Bot's live session is local to a computer: its T3 thread, its routines and the harness editing its memory files. If two computers ran the same Bot at once, both would fire its routines (duplicate runs) and both harnesses would edit the same files (conflicts), and each T3 thread would hold half the conversation. So each Bot has a **home machine** (default: where it was created), recorded in `m_active_device`. Every machine syncs and shows every Bot, and any machine can edit a Bot's definition and records, but only the home machine runs it. To move a Bot, the person presses **Run this Bot on this machine**:

1. The old machine stops starting new turns, lets the current one finish, pushes its last changes, and marks the Bot `released`.
2. The new machine pulls the folder, binds its own T3 project and thread, recreates the routines from `m_schedule`, and marks itself the home.
3. If the old machine is dead, the person can force it. If it comes back, it sees it is no longer home, stops the Bot locally, and turns anything it had not pushed into conflict copies.

Consequences: the new machine starts a fresh T3 main thread (T3 transcripts do not move), so handover seeds it with the Bot's memory and recent log. A single machine setup never sees any of this.

**Groups.** A group's messages are dispatched through the local T3 to each member's thread, so a group lives on one machine: every member's home must be that machine. The UI refuses to add a Bot whose home is elsewhere and warns when moving a member out of a group. Groups across machines (relaying messages through Anytype) are a later option.

**Clients.** A web or native client connects to one gateway. It lists every Bot and where each runs. Chatting with a Bot homed on another machine needs that machine's gateway, and `m_device.m_urls` carries its address so the client can offer "open on laptop".

**Routines.** T3 executes them. Definitions are `m_schedule` objects. A machine creates T3 tasks only for Bots it is home for and keeps the task ids locally, so there are no double runs.

**Clocks.** No correctness depends on wall clocks across machines. Content shas decide changes. Timestamps order append-only streams, with message ids as tie-break.

**New machine.** `hbots anytype pull`: set the host and key in Settings, register the device, choose which Bots to materialize, rebuild folders, regenerate derived files (`AGENTS.md`, `.mcp.json`, tool configs) with this machine's token and URL.

## Anytype features used

| Feature | Use | Phase |
|---|---|---|
| Type layouts (`profile`, `todo`, `note`, `bookmark`) | Bots look like profiles, tasks have the checkbox, links preview | P2 |
| Templates and `default_template` | Every new object starts from a skeleton, including the Bot overview page | P2 |
| Queries with views (table, list, gallery, kanban, calendar, graph) | Per Bot Memory, Logs, Agenda; task and project boards; graph of linked memory; conflicts board | P2, P8 |
| Collections | Hand ordered lists: Pinned Bots, team sidebar order | P8 |
| Sidebar widgets | Live Bots, Groups, Tasks, Conflicts, Devices, Inbox, Bin | P8 |
| Object links, mentions, backlinks, `anytype://` links | Memory notes link to Bots and records. `derived_from` and `supersedes` are real links | P5 |
| Date objects | Logs and schedules appear under their day | P5 |
| Bin and `restore` | "Undo delete" for Bots, records and notes. Garbage collection of tombstones | P4 |
| Files, images, PDFs | Attachments and generated media preview natively. Avatars as object icons | P4 |
| Inline `dataview` and `property` blocks | Bot overview page embeds its live queries (subject to S7) | P8 |
| Members and permissions | Read-only keys for viewers. Shared spaces later (`multi-user.md`) | later |
| Search `fields`, search stream | Cheap listings and the live change feed | P3 |
| Anytype MCP | Optional connector preset so a Bot can read the person's other spaces, separate from storage | optional |

Not used by design: chat objects, discussions, chat streams and chat status.

## Storage interface changes

Today's trait is path and bytes with `io::Result`. It cannot express revisions, appends, a remote store, a path policy or several machines.

1. **Revisions and compare-and-swap.** `stat(rel) -> Option<Stat { size, modified, rev }>`, `read_versioned`, `write_if(rel, bytes, expect: Option<Rev>) -> Result<Rev, Conflict>`. `LocalDisk` derives `rev` from sha256, `MemoryWorkspace` from a counter, the Anytype side from etag plus content sha. `docs/memory-maintenance.md` already needs this.
2. **Append.** `append(rel, bytes)`: `O_APPEND` locally, a segment or entry object remotely.
3. **Recursive, cursored list.** `list_prefix(rel, cursor)`.
4. **A real error type.** `WsError { NotFound, Conflict { current }, RateLimited { retry_after }, Offline, Forbidden, Denied, Io }`.
5. **Capabilities.** `capabilities() -> { cas, append, local_root, max_inline_bytes, sync }`.
6. **Path policy** as a wrapper, not a caller convention.
7. **Materialization.** `root_for_harness(slug)` keeps returning a local directory. `ensure_materialized(slug)` for first use on a machine.
8. **Finish routing raw filesystem calls through the trait** (`backends.md` phase 6, demanded by the memory-maintenance design): `bots.rs`, `attachments.rs`, `memory_tools.rs`, `face_icon.rs`, and the path based JSON stores `Rooms`, `Usage`, `ThreadMeta`, `Projects`, `RoutineHistory`. A `JsonDoc` layer over `Workspace` replaces each store owning a `PathBuf` and a `Mutex`.
9. **Narrow typed stores** as traits (`BotStore`, `RecordStore`, `RoomStore`, `ScheduleStore`, `SettingsStore`, `LocalState`). The default implementation keeps today's file format, so a plain local install is unchanged and `tests/compat` still passes.
10. **Storage settings model** (`storage_settings.rs`) that builds `Config` and chooses the `Workspace` and store implementations at start.
11. **Local state store** for `.bot/local.json`, `.botapp/groups.local.json`, device settings: never synced.

## CLI, migration and rollback

CLI: `hbots storage status | set`, `hbots anytype setup | status | doctor | import | pull | verify | conflicts | devices | handover <bot> | export`. Every command the Settings page offers has a CLI twin for headless installs.

1. `import --dry-run` lists what would be created and the estimated write time at the current limiter.
2. `import` walks the home in a fixed order (settings, bots, catalog, records, memory, skills, library, attachments, groups), idempotent per object (search before create, `Idempotency-Key`), resumable, and never sends anything the path policy denies.
3. `verify` compares shas file by file and field by field and changes nothing.
4. Rollback: stop the gateway and set the backend to File system, or run `export`. The working copy is intact. No step deletes the local tree.
5. Older gateways on the same home keep working in file system mode. They do not sync, and the scan picks their edits up the next time an Anytype gateway runs.

## Observability

`hbots anytype status` and `/api/sync/status` report state, outbox depth and age, last push and pull, stream health and reconnects, limiter state and 429 count, manifest size, open conflicts, history ring size and per peer lag. The gateway logs one line per sync decision at debug level (path, decision, shas) and never logs content or keys. A `sync.log` ring file under `~/.hbots/sync/` keeps the same lines for support.

## Local sync state

`~/.hbots/sync/` is machine-local state for the Anytype engine. It sits next to `storage.json` and `anytype-key`, outside the Bots folder (`HBOTS_ROOT`, default `~/.hbots/bots`). Copying the Bots folder does not copy it. An older gateway ignores the directory and keeps working in file system mode. Filesystem mode does not read it. The files are mode 0600 where the process creates them, and they hold no API key and no file bytes:

| File | What it is |
|---|---|
| `manifest.json` | Version 1. One entry per path: base sha, object id, remote etag and modified time, local size and mtime, tombstone flag |
| `device-id` | This gateway's stable id |
| `outbox.jsonl` | Writes waiting for Anytype. A warning at 1,000 items; new items are refused at 10,000 |
| `conflicts.json` | Open conflicts, and the sidecar path beside the file |
| `history/` | Per-path revisions. The last 20, or 14 days, whichever keeps more |
| `sync.log` | A small ring of decision lines (`path`, `decision`, shas). No content, no keys |

The API key stays in `~/.hbots/anytype-key` (or in `HBOTS_ANYTYPE_KEY` / `HBOTS_ANYTYPE_KEY_FILE`). It is not written under `sync/`.

## Test strategy

- **Unit:** codec (idempotence property tests, golden files from real memory and prompts, every block type above), path policy, three-way merge, limiter, slug collision rule, manifest transitions, handover state machine, settings validation and precedence, pairing flow with a fake server.
- **Conformance:** one suite run against `LocalDisk`, `MemoryWorkspace`, `SyncedWorkspace` over a mock Anytype, and a live scratch space.
- **Mock Anytype server** (`tests/mock-anytype.ts`, the pattern of `tests/mock-t3.ts`) for the endpoint subset, with switches for 409, 429, 410, archived, stream drops, and a second mock peer that writes concurrently with per property last-writer-wins, including torn writes, so the worst case is built in from the start.
- **Fault injection:** kill the gateway between disk write and push, between push and ack, during apply; drop the stream; revoke the key; lose the network; change the served OpenAPI; bad folder; bad host.
- **Contract:** a snapshot diff against the live `/v2/docs/openapi.json`, in CI against a recorded copy and manually against a live Anytype.
- **Client:** Settings tests in `app/app.test.contract` for locking, validation messages, restart banner, write-only key.
- **Existing:** the 340 gateway tests, `tests/e2e.sh` and `tests/compat` pass unchanged in file system mode in every phase.
- **Soak:** 10 Bots, 2,000 files, a day of edits, random kills, comparing final trees.

## Phases

Each phase is roughly one focused PR and ends with the criteria shown. Day counts are rough.

**P0. Spike, about 3 days, single Anytype instance.** Needs from the person: approve one pairing code in the desktop app, for a scoped key on a scratch space. Results are written back into this file.
- S1: the pairing flow end to end; scoped key behavior; `whoami` shape; space listing from a grant.
- S2: practical size of a text property and a file; creating 1,000 objects; real 429 numbers and headers; latency p50 and p95 for get, patch, search, upload.
- S3: codec round trip on real `MEMORY.md`, skills and prompts; block rendering rules; unknown block preservation.
- S4: edit in the app while the gateway patches; etag behavior with sync in flight; whether app block edits move `last_modified_date`.
- S5: files (extension, `files` property, range download, size).
- S6: search stream: events for create, patch, app edit, archive, restore; field filtering; snapshot size; reconnect; stream cap; heartbeat; app closed.
- S7: can `dataview` and `property` blocks, templates and widgets be authored through the API.
- S8: `Idempotency-Key` replay on object create and patch at the REST level, including after a restart.
- Exit: inline cap, limiter defaults, stream versus poll and block authoring are confirmed or changed, and the plan is updated.

**P1. Interface hardening and storage settings (file system), 6 to 7 days.** Extend `Workspace` (items 1 to 5 and 7), add `PathPolicy`, `JsonDoc`, `LocalState` and the typed store traits (local implementations only), route the remaining raw filesystem paths. Add `storage_settings.rs`, `storage.json`, env precedence and locking, `GET` and `PUT /api/storage`, `POST /api/storage/test`, `POST /api/storage/copy`, `POST /api/storage/restart`, the Settings Storage section with the folder field, validation, copy action and restart banner, and the Anytype option shown as "Not available yet". Exit: the 340 tests, `tests/e2e.sh` and `tests/compat` pass unchanged; changing the folder in Settings and restarting works on a copy of a real home; env variables lock the fields.

**P2. Anytype client, schema, connection settings, about 8 days.** `client.rs`, `pairing.rs`, `schema.rs` (types, layouts, icons, templates, queries, views from the declarative table), `m_meta`, `hbots anytype setup | status | doctor`, the mock server, and the Anytype fields in Settings: host (default `http://127.0.0.1:31009`), Connect with Anytype (pairing), advanced API key, space picker, Test connection. The backend still cannot be selected for use. Exit: bootstrap is idempotent and read-back keys are stored; 429, 409, 410, archived and drift each have a test; the key is write only and 0600; a person can pair, test and open the space in Anytype to see the empty structure.

**P3. Tier A, devices, conflicts, 10 to 12 days.** `AnytypeWorkspace`, `SyncedWorkspace`, manifest, outbox, change detection (stream and poll), tombstones, history ring, `m_device`, canary, `m_conflict`. The Anytype backend becomes selectable. Order: library and connectors, then `memory/`, `skills/`, attachments. Exit: conformance passes against the mock and a live scratch space; killing Anytype or the gateway at any step loses nothing; the concurrent writer mock never loses an unsynced local edit; `hbots anytype pull` rebuilds a Bot on an empty home.

Implementation sequence and acceptance criteria for P4 and P5: [Tier B/C plan](anytype-tier-bc-plan.md).

**P4. Typed stores, 9 to 11 days.** `m_bot`, `m_settings`, `m_model_profile`, `m_connector`, `m_schedule` reconciliation, `m_group` with message segments and the home machine rule, `m_attachment`, the eight record types, the local overlay, home machine and handover, slug collision rule, Bin based "undo delete". Retire the `library.json` blob and the `groups.json` messages array in Anytype mode (with migration). Exit: a record edited in Anytype shows in Michel within seconds on a stream; `bots.list()` stays within a small constant of today's cost with a warm cache; handover is covered by tests with two simulated machines.

**P5. Page-backed documents, 9 to 11 days.** `codec.rs`, `m_prompt`, `m_memory`, `m_skill`, `m_log`, links and dates, three-way merge on rendered markdown, unknown block sidecar. Exit: codec suites green on real files; edit a memory note in Anytype and see the file change under an idle Bot; edit the file and see the page change; a simultaneous edit yields a conflict object and both copies, never a loss.

**P6. Operations, 5 days.** `import`, `pull`, `verify`, `export`, `conflicts`, `devices`, `handover`; the Settings Sync block (state, outbox, devices, conflicts, handover); `GET /api/config` storage block; docs and a `PLAN.md` blurb. Exit: the migration section works end to end on a copy of a real home.

**P7. Remaining catalog types, 5 days.** `m_team`, `m_subagent`, `m_harness`, `m_computer`, `m_tool_profile`, `m_vault_grant`, `m_suggestion`, `m_approval`, `m_usage_day`, wired only where the gateway has a feature behind them.

**P8. Native Anytype experience, 4 days.** Collections, widgets, the Bot overview page with embedded views (S7 permitting), graph view of memory links, polished templates. Exit: opening the space on a phone shows Bots, boards and conflicts without any Michel UI.

**P9. Multi-instance validation, after the implementation, 3 to 4 days plus fixes.** Needs from the person: an invite link so a headless `anytype` bot account (already installed) can join the space.
- M1: the headless build serves `/v2`; scoped key; joining by invite.
- M2: two instances write the same property and different properties of one object at once. What survives and how long until both agree.
- M3: cross-instance lag for create, patch, file upload, archive.
- M4: key name semantics: can a key named `hbots` on one instance purge an object created by a key named `hbots` on another.
- M5: can one headless account serve several machines.
- M6: soak with two real instances.
- Exit: results recorded here; the mock peer is adjusted to match reality; any needed rework is scheduled.

Rough total: 13 to 15 weeks for P0 to P8, plus P9. P3 to P5 are the bulk. P0 or P9 can change the plan.

## Interactions with other designs

- `docs/memory-maintenance.md`: needs revisions, journals and `Workspace` only. P1 satisfies it, P3 makes memory Anytype capable, P5 adds categories, valid ranges and provenance.
- `docs/backends.md` phase 6 is P1 here. Its harness abstraction is independent. Its "145 tests" figure is stale.
- `docs/bot-execution-environment.md`: "which machine's gateway is home for this Bot" and "which linked T3 computer runs it" are different axes. `m_device` and `m_computer` are separate on purpose. Reconcile before P4 so "where it runs" has one meaning in the UI. That design also adds Settings controls, so the Storage section and the computer setting must share one Settings layout.
- `docs/multi-user.md`: revisit before sharing the space with other people.

## Risks

- **v2 is pre-release.** Thin client, small subset, contract test, `api_changed` degraded mode.
- **Concurrent edits across machines are merged by Anytype.** Home machine, append only streams, content authority, the history ring and a worst case mock bound the damage. Because validation is deferred to phase 9, a surprise there may force rework of P3 to P5.
- **Handover changes T3 continuity.** A moved Bot starts a new main thread. Documented and seeded from memory.
- **Anytype must run on every machine.** The working copy keeps Bots working and peer lag makes the gap visible.
- **Codec fidelity.** Normalization is accepted only if idempotent and tested, with unknown blocks preserved. Opaque files stay exact.
- **Stream limits.** Unknown cap. One stream per gateway with polling fallback.
- **Schema churn.** Keys are permanent, types and properties cannot be purged, select options cannot be deleted. Migrations are additive and tested.
- **Settings misconfiguration.** A bad folder or host never blocks start. The gateway falls back to the file system working copy and says why.
- **Object count.** One object per file and per record is clean in the app only with queries and widgets (P2, P8).
- **Scope creep.** Many catalog types have no gateway behavior yet and are created lazily (P7).
- **Shared space exposure.** Anyone in the space reads everything and can write objects machines pull. Setup refuses a shared space by default.

## Open questions

1. **Model availability per machine.** `m_model_profile` may name a provider instance that a given machine lacks. Warn and fall back to the machine's default, or refuse to run the Bot there?
2. **Execution designs.** Reconcile `m_computer` and `m_device` with `bot-execution-environment.md`.
3. **Vault across machines.** Out of scope. If wanted it needs its own design (ciphertext only, key distribution).
4. **Home machine and groups for v1.** Confirm the plain rule above: each Bot runs on one machine at a time, and a group's members all live on that machine. If you want Bots from several machines in one group, that is the relay design, a larger step.
