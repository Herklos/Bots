# Anytype sync: implementation plan for an agent

Status: ready to implement (2026-11-12). Written against the code at commit `495be9a`.

This file is the executable slice of [anytype-backend.md](anytype-backend.md) (the design, audit and decisions) and [anytype-schema.md](anytype-schema.md) (the type catalog). Read both first, then this one. Where this file and the design disagree, this file wins, because it was checked against the current code. Section "Differences from the design" lists each one.

## 0. Goal and scope

**Goal.** When the person picks Anytype in Settings → Storage, Bot data really lives in their Anytype space. Anytype is the authority. The folder under `HBOTS_ROOT` (default `~/.hbots/bots`) stays as the local working copy, because the harness (T3 Code running Claude, Codex, ...) needs a real directory and edits files with its own tools. The gateway keeps that folder and the Anytype space in agreement, in both directions, with no silent data loss.

**In scope (v1, milestones M0 to M7).**

1. A hardened Anytype HTTP client (limits, retries, etags, idempotency).
2. Idempotent schema bootstrap of four types: `hb_meta`, `hb_file`, `hb_conflict`, `hb_device`.
3. A sync engine for **tier A**: byte-exact files under an allow list (`memory/**`, `skills/**`, `library/**`, `.bot/attachments/**`, `.bot/library.json`, `.botapp/shared-memory.md`). Push, pull, three-way merge, tombstones, conflicts, history, crash safety, offline queue.
4. Change detection: disk scan plus Anytype poll (v1), search stream (M6).
5. Wiring: the backend actually becomes "Anytype", a status API, a Settings Sync block, a `SyncedWorkspace` so `Library` writes notify the engine.
6. Tests: a mock Anytype server with a concurrent peer, fault injection, a live smoke run.

**Out of scope for this plan (later phases of the design).** Typed stores (`hb_bot`, `hb_group`, records, settings objects), page-backed documents with the markdown codec (P5), catalog types, widgets and collections, multi-machine home-machine rules, import/export CLI, multi-instance validation (P9). Where v1 must leave a seam for them, the plan says so.

**Non-goals.** Syncing secrets, T3 transcripts, `bot.json` runtime fields, or derived files (see the path policy in 5.2). Using the Anytype MCP. Using Anytype chat features.

## 1. Current state of the code (verified)

Do not re-derive these. They were read from the source.

| Area | State | Where |
|---|---|---|
| Storage choice, env locks, `storage.json`, key file, pairing, check | Done and shipped | `gateway/src/storage/settings/*` |
| Settings UI: backend radio, folder, Anytype connect, space name | Done. Anytype tab enables only when the space is found | `app/app.contract` (section `settings.storage`), `app/app.ts` (`loadStorage`, `saveStorage`, `saveSpace`, `checkAnytype`) |
| Anytype client | Pairing, `whoami`, `GET /v2/spaces`, members only. **No object, type, property, search or stream calls.** `reqwest` client built per call, 10 s timeout, `Anytype-Version: 2025-11-08` | `gateway/src/storage/adapters/anytype/client.rs` (130 lines), `tests.rs` (81 lines, axum mock) |
| Effective backend | **Always `filesystem`.** `view.rs` hardcodes `"backend": Backend::Filesystem` under `effective` and `"available": ["filesystem"]`. A saved `anytype` is stored but nothing reads it | `gateway/src/storage/settings/view.rs:28,48` |
| `anytype_status()` | Stub returning `{available:false,status:"unavailable"}` | `gateway/src/storage/settings/anytype.rs` |
| `Workspace` trait | `read`, `write`, `list`, `remove`, `root_for_harness`. No revisions, no append, no stat, `io::Result` errors | `gateway/src/storage/workspace.rs` (55 lines) |
| `Workspace` users | **Only `Library`** (`bots/library/{mod,connectors}.rs`). Built in `main.rs:136` as `Arc::new(storage::LocalDisk::new(cfg.bots_root.clone()))` | |
| Raw filesystem use | Everything else bypasses the trait: `bots/{mod,lifecycle,instructions,face_icon,icons,memory_tools,agent_context,subagent_tools}.rs`, `chat/{attachments,thread_meta,rooms,routines}`, `projects/mod.rs`, `bots/usage`, `vault`. About 24 files | `grep -rnE "tokio::fs|std::fs"` |
| Bot folder layout | `<root>/<slug>/{AGENTS.md, CLAUDE.md (symlink), .mcp.json, .codex/, .claude/, memory/, skills/, library/, .bot/{bot.json, library.json, connectors.json, attachments/}}`, app state in `<root>/.botapp/` | `bots/lifecycle.rs`, `bots/instructions.rs` |
| `Bots::load()` | Rescans `read_dir(root)` and reads every `bot.json` on each call, no cache | `bots/mod.rs:67` |
| Config | `Config { data_root (~/.hbots), bots_root, storage: ProcessEnv }`. `bots_root` is resolved from env, then `storage.json`, then default | `platform/config.rs` |
| Atomic write | `storage::write_atomic(path, bytes, mode)` (tmp file, fsync, rename) | `storage/adapters/filesystem/atomic.rs` |
| Folder copy with hash check | `copy.rs` already walks and hashes a tree. Reuse its walker | `storage/adapters/filesystem/copy.rs` |
| Dependencies | `reqwest 0.12` (json, rustls, **no `stream` feature**), `sha2`, `hex`, `uuid`, `futures-util`, `tokio-stream`, `async-trait`, `url`, `rand`. No file watcher, no diff crate | `gateway/Cargo.toml` |
| Lints and CI | Workspace lints: `unsafe_code=forbid`, clippy `-D warnings` incl. `unreachable_pub`, `map_err_ignore`, `unused_async`, `allow_attributes_without_reason`, cast lints. `cargo fmt --check`. **500 line cap per `.rs` file** (tests included) via `scripts/check-rs-length.sh`. `cargo test --locked` | `Cargo.toml`, `.github/workflows/ci.yml` |
| Other test harnesses | `tests/e2e.sh`, `tests/compat/run.sh` (persisted-data compatibility), `tests/mock-t3.ts` | |
| Repo skills to read before touching the matching area | `.agents/skills/compatibility/SKILL.md` (any new persisted file), `exact-client/SKILL.md` (UI and strings), `dev-watch/SKILL.md` (running it) | `AGENTS.md` |

Anytype side: the local desktop app serves the API at `http://127.0.0.1:31009`. Pairing, key storage (`~/.hbots/anytype-key`, mode 600), and the space lookup by name already work (`docs/anytype-spike.md`, S1).

## 2. Architecture decision

The design doc (decision "SyncedWorkspace over LocalDisk") assumed all storage goes through `Workspace`. The code says it does not: only `Library` does, and about 24 files write the folder directly. Routing all of them through a trait is a large refactor that blocks the feature.

**Decision: a filesystem-mirroring sync engine.** The working copy stays a plain folder that any code and the harness may touch. A background engine reconciles that folder with Anytype. No existing module needs to change for files to sync.

```
 gateway code ─┐
 harness (T3) ─┼─► working copy folder  <──scan──┐
 Library ──────┘   (HBOTS_ROOT)                  │
   │ SyncedWorkspace.write() ──notify──► [ Sync engine ] ──► Anytype client ──► local Anytype ──► Anytype sync
                                           manifest · outbox · history · conflicts
```

- The engine **scans** the allow-listed part of the folder (mtime and size shortcut, sha256 on change), so it sees every writer. It does not depend on a watcher crate.
- `SyncedWorkspace` is a thin `Workspace` wrapper over `LocalDisk` whose only extra job is `engine.nudge(path)` after a write or remove, so `Library` changes sync within a second instead of on the next scan. It is optional polish (M5) and never required for correctness.
- Anytype being unreachable never blocks anything. The folder is always the working truth for the running process.
- Typed stores, the codec and the trait extensions of the design stay possible later. The engine's per-path `MergePolicy` is the seam for page-backed documents.

## 3. File and module layout

Follow AGENTS.md: one file per concern, `adapters/anytype/` for Anytype-specific code, `tests.rs` or `tests/` siblings, every file at most 500 lines.

```
gateway/src/storage/
  adapters/anytype/
    mod.rs            re-exports
    client/           (split the current client.rs into a folder)
      mod.rs          AnytypeClient { host, key, space_id }, builder, shared reqwest::Client
      http.rs         request helper: auth header, version header, Idempotency-Key, If-Match, error mapping
      error.rs        AnytypeError { Unreachable, NotGranted, EtagMismatch{current}, RateLimited{retry_after}, ObjectArchived, Gone, ApiChanged, Bad{status,msg} }
      limiter.rs      write rate limiter (token bucket), adaptive on 429
      pairing.rs      existing start_pairing, confirm_pairing (moved)
      status.rs       existing status() (moved)
      objects.rs      create, get, patch, delete, archive, restore
      search.rs       search with filters, fields projection, pagination walker
      schema.rs       list/create types and properties
      files.rs        upload, download (M0 decides the shape)
      stream.rs       SSE search stream (M6)
    schema/           declarative table + bootstrap
      table.rs        the type and property declarations (hb_meta, hb_file, hb_conflict, hb_device)
      bootstrap.rs    idempotent create, read back keys, write hb_meta
    tests/            mock server and client tests
      mock/           in-memory Anytype: types, properties, objects, search, 409/429/archived switches, peer writer
      client.rs, schema.rs
  sync/               NEW domain folder (backend independent logic)
    mod.rs            SyncHandle (nudge, status, run_once), start()
    policy.rs         PathPolicy: allow, deny, MergePolicy per path
    manifest.rs       Manifest + persistence (~/.hbots/sync/manifest.json)
    scan.rs           local scan to LocalState map
    remote.rs         remote listing to RemoteState map, via search
    decide.rs         pure function: (base, local, remote) -> Action
    apply.rs          perform an Action (push, pull, delete, conflict)
    merge.rs          text three-way merge
    conflict.rs       conflict records and resolution
    history.rs        history ring under ~/.hbots/sync/history/
    outbox.rs         durable queue of pending remote writes
    engine.rs         loop: scan, list, decide, apply, backoff, status
    status.rs         SyncStatus snapshot for the API
    tests/            one file per topic
  adapters/anytype/workspace.rs   SyncedWorkspace (M5)
  settings/sync_http.rs           GET /api/sync/status, POST /api/sync/run, conflict routes
```

Decide `storage/sync/` vs a top-level `sync/`. Use `storage/sync/` (it is the storage domain). Register routes in `api/router/mod.rs` next to the existing `/api/storage*` routes.

## 4. Data model on the Anytype side

Only the four types v1 needs. Keys contain **no digits** and are passed explicitly (design rule 1: Anytype normalizes keys, and `hbprobe2` became `hbprobe_2`). After creating, always read the key back and store the mapping in `hb_meta`.

Properties (format in parentheses): `hb_path` (text), `hb_dir` (text), `hb_content` (text), `hb_sha` (text), `hb_size` (number), `hb_mime` (text), `hb_mtime` (number), `hb_deleted` (checkbox), `hb_owner_bot` (text), `hb_device` (text), `hb_rev` (number), `hb_blob` (files), `hb_schema_version` (number), `hb_min_gateway` (text), `hb_keys_json` (text), `hb_conflict_state` (select: `open`, `resolved`, `dismissed`), `hb_device_a`, `hb_device_b`, `hb_sha_a`, `hb_sha_b` (text), `hb_detected_at` (date), `hb_copy_a`, `hb_copy_b` (files), `hb_slug` (text), `hb_hostname`, `hb_gateway_version`, `hb_status` (text), `hb_last_seen` (number).

Types:

| Type key | Layout | Purpose | Properties |
|---|---|---|---|
| `hb_meta` | basic | Singleton: schema version, key map | `hb_schema_version`, `hb_min_gateway`, `hb_keys_json` |
| `hb_file` | basic | One synced path, byte exact | `hb_path`, `hb_dir`, `hb_content`, `hb_blob`, `hb_sha`, `hb_size`, `hb_mime`, `hb_mtime`, `hb_deleted`, `hb_owner_bot`, `hb_device`, `hb_rev` |
| `hb_conflict` | basic | An unresolved disagreement | `hb_path`, `hb_owner_bot`, `hb_conflict_state`, `hb_device_a`, `hb_device_b`, `hb_sha_a`, `hb_sha_b`, `hb_copy_a`, `hb_copy_b`, `hb_detected_at` |
| `hb_device` | basic | One gateway on one machine | `hb_slug`, `hb_hostname`, `hb_gateway_version`, `hb_status`, `hb_last_seen` |

Rules for `hb_file` (all from the design audit):

- **Object name** = the rel path (for example `ada/memory/MEMORY.md`), so a person sees something readable in Anytype. **Identity is `hb_path`**, not the name.
- `hb_path` is the rel path under `HBOTS_ROOT` using `/`. It must pass `safe_rel` and the allow list on every pull (a pulled path is untrusted input).
- Text content that is valid UTF-8 and at most **256 KiB** goes in `hb_content`. Anything larger or binary goes in `hb_blob` as an Anytype file and `hb_content` stays empty. `hb_sha` is always the sha256 of the **raw bytes**, lowercase hex.
- **`hb_sha` and `hb_content` are not trusted together** (a concurrent property merge can pair one machine's content with another's sha). On every read the engine recomputes the sha from the content it received and uses that. `hb_sha`, `hb_rev`, `hb_device`, `hb_mtime` are advisory.
- **Delete = tombstone**: set `hb_deleted=true` and clear `hb_content`. Never archive on delete, because an archived object disappears from search and cannot be edited (409 `object_archived`). Garbage collection of tombstones older than 30 days (archive to Bin) is M7.
- **Duplicates**: two `hb_file` objects with one `hb_path` can appear after a crash or a race. Keep the earliest `created_date`, merge into it, tombstone-archive the other (M5).
- Exactly one `hb_meta` object. If several exist, use the earliest.

Anytype limits the engine must respect: string 1 MiB, structured body 1 MiB, 128 properties, writes rate limited (429 on mutating routes only, documented sustained about 1/s, burst 60). Reads are not limited.

## 5. Sync engine design

### 5.1 State the engine keeps (all local, never synced)

Directory `~/.hbots/sync/` (under `data_root`, outside the Bots root so copying or changing the folder cannot carry a stale manifest).

- `manifest.json`: `{ version, host, space_id, root, device_id, entries: { "<rel>": { base_sha, object_id, remote_etag, remote_modified, local_mtime_ns, local_size, tombstone } } }`. If `host`, `space_id` or `root` differ from the current config, the manifest is discarded and the next run is a first sync (4.7).
- `device-id`: a uuid created once.
- `outbox.jsonl`: one JSON line per pending remote write `{ id, path, op, sha, idem_key, attempts, next_at }`. Written before the remote call, removed after the acknowledgement. Replays are safe because every mutation carries a stable `Idempotency-Key = sha256(device_id | path | op | sha)`.
- `history/<sha-of-path>/<rev>.bin`: copies of every version this device overwrites or removes (last 20 per path or 14 days, whichever keeps more). This is the safety net for the one race that cannot be removed.
- `conflicts.json`: local index of open conflicts for the UI (the `hb_conflict` object is the shared record).

All files written with `storage::write_atomic`. Mode 0600 for the manifest and outbox.

### 5.2 Path policy

`policy.rs` exports `fn classify(rel: &str) -> Class` where `Class` is `Allow { merge: MergePolicy }` or `Deny`. A rel is `<slug>/...` or `.botapp/...`.

Allow (per Bot): `memory/**`, `skills/**`, `library/**`, `.bot/attachments/**`, `.bot/library.json`. Shared: `.botapp/shared-memory.md`.

Deny, always, checked on push **and** on every pulled path, with symlinks refused (`symlink_metadata`, never follow):
`AGENTS.md`, `CLAUDE.md`, `.mcp.json`, `.codex/**`, `.claude/**`, `.bot/bot.json`, `.bot/local.json`, `.bot/connectors.json`, `.botapp/**` except `shared-memory.md`, `.git/**`, `*.conflict-*`, `*.tmp`, `*.lock`, anything outside the root, and the secret files by name (`vault.enc`, `.vault-key`, `access.json`, `session-secret`, `.bot-mcp-token`, `t3-token`, `.env`, `storage.json`, `anytype-key`).

Why `bot.json` is denied in v1: it mixes shared fields with the T3 binding (`projectId`, `mainThreadId`), `workspaceRoot` and `lastSeenAt`, which differ per machine. Syncing it needs the typed `hb_bot` store (later phase) or a split into `bot.json` and `local.json`. A second machine therefore gets files, not Bots, until that phase. State this plainly in the Settings copy (see 8).

`MergePolicy`: `Text3Way` for `*.md`, `*.txt`, `*.html`; `Whole` (no merge, conflict if both sides changed) for `*.json` and binaries. `.bot/library.json` is `Whole` because a JSON blob cannot be merged by lines safely.

### 5.3 One reconcile cycle

`engine.run_once()`:

1. **Scan local.** Walk the allow-listed paths under `bots_root` (reuse the walker idea in `filesystem/copy.rs`). For each file take `(size, mtime_ns)`. If equal to the manifest's cached values, reuse `base_sha` as the current sha. Otherwise hash it. Skip a file modified within the last 1 s (a writer may be mid-write) and pick it up next cycle. Result: `LocalState = { rel -> Present{sha,size,mtime} | Absent }`.
2. **List remote.** Search `hb_file` objects in the space with the `fields` projection `hb_path, hb_sha, hb_deleted, hb_device, hb_rev, last_modified_date, hb_size` (no content). Paginate with the walker. Result: `RemoteState = { rel -> Present{object_id, etag?, sha, modified} | Tombstone{...} | Absent }`. If the same `hb_path` appears twice, pick the earliest `created_date` as the object and schedule duplicate cleanup.
3. **Decide per path** with the pure function `decide(base, local, remote) -> Action` (table below). Unit-test it exhaustively.
4. **Apply** actions in a fixed order: pulls and local deletes first, then pushes, bounded per cycle (default 50 remote writes) so a first sync of a large tree is incremental.
5. **Update manifest** after each acknowledged action, not at the end, so a crash resumes.

For a remote path whose cheap fields suggest a change (`hb_sha` differs from the manifest, or `last_modified_date` is newer than `remote_modified`), fetch the full object (content) before deciding. A newer `last_modified_date` with an unchanged `hb_sha` means a person edited in the app: recompute the sha from the fetched content, because the advisory fields are stale (design finding A12).

### 5.4 The decision table

`base` is `manifest.entries[rel].base_sha` (None if never synced). `L` is the local sha (None if absent), `R` the remote sha (None if absent or tombstone).

| base | L | R | Meaning | Action |
|---|---|---|---|---|
| None | None | None | nothing | no-op |
| None | some | None | new local file | **Push create** |
| None | None | some | new remote file | **Pull create** (write file atomically, set base) |
| None | some | some, same sha | both created the same | adopt: set base, no I/O |
| None | some | some, differ | both created different | **Conflict** (first-sync rule 4.7) |
| b | b | b | unchanged | no-op |
| b | L≠b | b | local edited | **Push update** (If-Match with the stored etag) |
| b | b | R≠b | remote edited | **Pull update**, only if local still equals b right before the rename |
| b | L≠b | R≠b, L=R | both made the same edit | adopt |
| b | L≠b | R≠b, L≠R | both edited | `Text3Way`: merge. Clean: write merged to both. Otherwise **Conflict**. `Whole`: **Conflict** |
| b | None | b | local deleted | **Push tombstone** |
| b | b | tombstone | remote deleted | **Delete local** (move a copy to history first) |
| b | None | tombstone | both deleted | drop manifest entry |
| b | L≠b | tombstone | local edited, remote deleted | **Edit wins**: push update with `hb_deleted=false` |
| b | None | R≠b | local deleted, remote edited | **Edit wins**: pull create |

Edit-wins on delete versus edit is deliberate: the safe error is a file coming back, never a file lost.

### 5.5 Applying actions

- **Push create.** Search `hb_path` first (a crash between create and ack must not duplicate), else `POST objects` with `Idempotency-Key`. Store `object_id`, `etag`, `base_sha`.
- **Push update.** `PATCH` with `If-Match: <etag>` and the new properties. On `409 etag_mismatch`: re-read the object, recompute its sha, and re-enter `decide` (remote equals base: retry; remote equals ours: done; otherwise conflict). At most 3 attempts per cycle.
- **Pull.** Write through `write_atomic` after re-hashing the local file immediately before the rename. If it changed since the scan, abort this path (it becomes a conflict candidate next cycle). Preserve the executable bit only if the file had it locally. Create parent directories.
- **Binary and large.** Upload via the files API (M0 fixes the exact call), then set `hb_blob`. On pull, download and verify the sha before replacing.
- **Delete local.** Copy to history, then `remove_file`. Never `remove_dir_all` from the engine.
- **Every overwrite or delete of local content first stores the old bytes in the history ring.**

### 5.6 Conflicts

On a true conflict:

1. Keep the **local file as is** on disk (the harness may be using it).
2. Create or update an `hb_conflict` object (state `open`) with `hb_path`, both shas and both versions as files (`hb_copy_a` local, `hb_copy_b` remote).
3. Write the remote version beside the local one as `<name>.conflict-<device8>-<utc>.<ext>` in the same folder so the person can diff with their normal tools. The path policy denies `*.conflict-*`, so these never sync.
4. Record it in `conflicts.json` and in the status API. Stop syncing that path until resolved.

Resolution (`POST /api/sync/conflicts/{id}/resolve` with `keep: "local" | "remote" | "both"`): `local` pushes the local content over the remote and sets the base; `remote` pulls the remote content over the local file (old local to history); `both` keeps the local file, renames the remote copy to a permanent sibling and treats it as a normal new file. Then mark the `hb_conflict` object `resolved`, delete the `.conflict-*` file.

Resolving in the Anytype app (setting `hb_conflict_state` to `resolved` or `dismissed`) is detected by the next remote listing and clears the local entry.

### 5.7 First sync (the moment Anytype is switched on)

State machine stored in the manifest: `needs_import` → `importing` → `live`.

1. Switching the backend to Anytype saves the choice (already done) and, after the restart, starts the engine in `needs_import`. **Nothing is pushed yet.**
2. Settings shows a dry run: counts of files that would be pushed, pulled, conflicting, and the estimated time at the current write limit. `POST /api/sync/import {dryRun:true}`.
3. The person confirms. `POST /api/sync/import {dryRun:false}` runs the normal cycle with an empty base, ordered: shared memory, then each Bot's `memory/`, `skills/`, `library/`, attachments.
4. **Both sides have the same path with different content (empty base):** conflict, never an automatic choice.
5. Rollback is always "set the backend to File system". No step deletes the folder.

### 5.8 Failure handling

| Failure | Behavior |
|---|---|
| Anytype not running at start | Gateway starts, serves the folder, engine state `offline`, retry with exponential backoff (2 s up to 60 s). Pending pushes stay in the outbox, which is capped (warn at 1,000 items, refuse new items at 10,000 and report) |
| 429 | Honor `Retry-After`, halve the limiter rate, requeue |
| 409 `etag_mismatch` | Re-read and re-decide (5.5) |
| 409 `object_archived` | Restore it if our change is newer than the archive, otherwise treat as a delete |
| 404 or 410 on an object | Treat as purged: drop `object_id` and re-run `decide` (it becomes a create if the local file still exists) |
| 401 or 403 | State `not_granted`, banner in Settings, no retries until the person reconnects |
| Response shape drift (missing expected fields) | State `api_changed`, read-only: pulls continue, pushes pause |
| Process killed mid-cycle | The outbox and manifest were written before and after each remote call. Restart re-runs the cycle. Idempotency keys and the search-before-create make it safe |
| Local file vanishes between scan and apply | Re-hash, re-decide on the next cycle |
| Disk full or permission error on a pull | Record per-path error, skip, surface in status |
| Gateway cannot reach the space (renamed, deleted) | State `not_found`, no writes, message in Settings |

Never panic on remote data. A malformed object is skipped and counted.

### 5.9 Loop, concurrency, rate

- One engine task per gateway (`tokio::spawn` from `start()`), driven by: a 3 s scan timer, a 10 s remote poll timer, `nudge(path)`, and "Sync now". A `tokio::sync::Mutex` guards `run_once` so cycles never overlap.
- The write limiter is a token bucket (default 0.8 writes per second, burst 40) shared by the whole client. Reads are unthrottled but the listing is paginated at 100 and uses the `fields` projection.
- Cycle budget: at most 50 remote writes and 200 ms of CPU hashing per cycle, resumed next cycle.
- The engine takes no lock on the folder. Correctness comes from re-hashing right before every local write.

## 6. Milestones

Each milestone is one PR. After each: `cargo fmt --all`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `sh scripts/check-rs-length.sh`, and `tests/e2e.sh` plus `tests/compat/run.sh` unchanged in filesystem mode. New persisted files follow `.agents/skills/compatibility/SKILL.md`.

### M0. Live spike (about 2 days, needs the person)

Purpose: replace guesses with measured facts before writing the engine. Use a **scratch space**, never the person's real data. **Warning, from the design audit: types and properties cannot be purged through the API, so every type created in a space stays there.** Do the schema experiments in a throwaway space (the design names `Test-AnyBot`) and confirm with the person before creating anything in the real `hbots` space.

Needs from the person: one pairing approval for a scoped key on the scratch space, Anytype Desktop running.

Questions to answer, and write the answers into `docs/anytype-spike.md` (S2 onward):

1. **Request shapes.** Fetch `GET /v2/docs/openapi.json` from the running app and record the exact bodies for: create type, create property, create object (with properties and `body`), patch object, search (filters, `fields`, pagination, sort), delete or archive, restore. Do not trust the shapes quoted in this plan.
2. **Property limits.** Largest text property that round-trips byte exact (design says 1 MiB). Confirm 256 KiB is safe and fast.
3. **Files.** Exact upload (multipart?) and download calls, size limit, what the stored name and mime become, how `hb_blob` references it, ranged download.
4. **Etags.** Confirm `If-Match` semantics and that the response returns the new etag on create and patch.
5. **`Idempotency-Key`.** Replay on create and on patch, including after a gateway restart.
6. **Rate limiting.** Real 429 numbers and `Retry-After` header on a burst of 100 creates.
7. **Latency.** p50 and p95 for get, patch, search (100 results), file upload.
8. **Change detection.** Does editing a text property or a body in the Anytype app change `last_modified_date`? Is a `last_modified_date` filter usable in search? This decides poll cursors.
9. **Search stream.** Whether `POST /v2/spaces/{id}/search/stream` works on this build, event shapes, heartbeat, and what happens when the app closes. (Needed for M6, not M1 to M5.)
10. **Key normalization.** Confirm explicit keys with no digits survive unchanged, and what the response returns.
11. **Archive and tombstone behavior** as listed in design audit A3, once, to confirm.

Exit: a short result table in `docs/anytype-spike.md`; the "Unknown" paragraph of the design doc is updated; this plan's section 4 and 5 are amended wherever a result contradicts it.

### M1. Client hardening and mock server (about 4 days)

- Split `client.rs` into `client/` per section 3. Keep the public names used by `settings/anytype.rs` (`start_pairing`, `confirm_pairing`, `status`) working; update `adapters/anytype/mod.rs` re-exports.
- Add `AnytypeClient::new(host, key, space_id)` holding one shared `reqwest::Client` (connection reuse), `limiter`, and the error enum.
- Implement `objects.rs`, `search.rs` (with the pagination walker), `schema.rs`, `files.rs` per the M0 results. Every mutation takes an explicit `idem_key`.
- Add `reqwest`'s `stream` feature only in M6.
- Build the **mock Anytype server** under `adapters/anytype/tests/mock/`: in-memory types, properties, objects, files, search with filters and `fields`, etags, `Idempotency-Key` replay, and switches to force: 409 etag mismatch, 429 with `Retry-After`, 404/410, archived, malformed body, slow response. Include a **peer writer** API that mutates an object as if from another machine, with per-property last-writer-wins and an option to tear a write (new `hb_content`, old `hb_sha`).
- Tests: every error maps to the right `AnytypeError`; limiter shrinks on 429; replay with the same idempotency key does not duplicate.
- Exit: `cargo test` green; no behavior change in the app.

### M2. Schema bootstrap (about 3 days)

- `schema/table.rs`: the declarative table of section 4. One place defines keys, names, formats, layout.
- `schema/bootstrap.rs`: list existing types and properties, create what is missing, **never rename or delete**, read the actual keys back, write them to the single `hb_meta` object along with `hb_schema_version=1` and `hb_min_gateway=<CARGO_PKG_VERSION>`. Re-running is a no-op.
- Readers tolerate unknown properties. Writers never send an option a select property does not hold.
- Route `POST /api/storage/anytype/setup` (local-only like the other Anytype routes, uses `ensure_local`): runs bootstrap with `dryRun` support and returns what it would create.
- Settings: a "Prepare space" button shown when the space is found but `hb_meta` is absent, and the existing check shows "Ready" once the schema is present. Strings in `app/strings/en.json` and `fr.json`.
- Tests: bootstrap against the mock twice (idempotent); a space missing one property gets only that property; a drifted key is honored via `hb_keys_json`.
- Exit: against a scratch space, setup creates the four types once and Anytype shows them.

### M3. Sync core, local half (about 5 days)

Build everything that does not touch the network, with fakes.

- `policy.rs` with the allow and deny rules and a test per rule (including symlink refusal and `..` in a pulled path).
- `manifest.rs`: load, save (atomic, 0600), invalidation on host, space or root change, migration-safe versioning.
- `scan.rs`: the scan with the mtime and size shortcut, the 1 s settle rule, and a test that a changed file is rehashed and an unchanged one is not.
- `decide.rs`: the pure table of 5.4 as a function. One test per row, plus a property test that `decide` never returns a destructive action when `base` is None.
- `merge.rs`: add the `diffy` crate (check `cargo add diffy --dry-run`; if unsuitable use `similar`) for line three-way merge. Tests: clean merge, overlapping edit gives conflict, identical edits merge, CRLF preserved.
- `history.rs` and `outbox.rs` with tests for the ring limits and replay.
- Exit: all of the above unit tested with no HTTP.

### M4. Sync core, network half (about 6 days)

- `remote.rs` (listing), `apply.rs` (5.5), `conflict.rs` (5.6), `engine.rs` (5.3, 5.9), `status.rs`.
- First-sync state machine (5.7) with dry run.
- Engine tests against the mock, one file per topic under `sync/tests/`: push create, push update, pull create, pull update, delete both ways, edit-wins on delete, both edited (clean and conflict), first sync with overlapping paths, binary and large file, duplicate `hb_path`, tombstone resurrect, archived restore, etag retry, 429 backoff, offline then reconnect, outbox replay.
- **Fault injection** via a hook trait the engine calls between steps (`BeforeRemoteCall`, `AfterRemoteCall`, `BeforeManifestWrite`): kill the engine at each point, restart, assert no loss and no duplicate objects.
- **Concurrency test**: two engines with two temp folders and two device ids against one mock, running random edits for a few hundred cycles. Assert the invariant: **no write that was acknowledged locally is ever lost**; after both quiesce, either both folders hold identical content or an open conflict exists for the path.
- Exit: the invariant test passes with 1,000 random operations over 50 paths.

### M5. Wiring into the gateway (about 5 days)

- `storage/sync/mod.rs::start(state) -> Option<SyncHandle>` returns None for the filesystem backend. Called from `main.rs` after `AppState` is built. A bad host, missing key or missing space logs a warning, records the reason, and leaves the engine off. **Never fail startup.**
- `view.rs`: `effective.backend` becomes `"anytype"` when the engine is running and `"filesystem"` otherwise, with `effective.reason` explaining a fallback. `available` lists both. Replace the `anytype_status()` stub with the real snapshot (`state`, `lastSync`, `outbox`, `conflicts`, `peers`).
- Routes (all behind the normal session, refused through remote access like the other storage routes): `GET /api/sync/status`, `POST /api/sync/run`, `POST /api/sync/import`, `GET /api/sync/conflicts`, `POST /api/sync/conflicts/{id}/resolve`. Add to `api/router/mod.rs`. Keep each handler file under 500 lines.
- `SyncedWorkspace` (`adapters/anytype/workspace.rs`): wraps `LocalDisk`, delegates everything, calls `engine.nudge(rel)` after `write` and `remove`. Swap it into `main.rs:136` only when the engine is running.
- Device registration: create or update this machine's `hb_device` object (`hb_slug` = device id, hostname, version, `hb_last_seen` every 60 s). Read the others to show peers and lag.
- Restart semantics: switching the backend still says "restart required" (the existing banner). Do not hot-swap the engine.
- Settings UI (`app/app.contract`, strings in `en.json` and `fr.json`, tests in `app/app.test.contract`): a **Sync** card shown when Anytype is the active backend: state chip, last sync time, pending count, devices, a "Sync now" button, the import dry-run and confirm, and the conflict list with Keep mine, Keep Anytype and Keep both. Follow the card, label and button styles in the existing storage section (`FieldLabel`, `ActionMain`, `ActionQuiet`, `GroupCard`). Add a plain note: "Bots, prompts and settings do not sync yet. Memory, skills and library files do."
- `bots/library`: nothing changes except it now holds `SyncedWorkspace`.
- Exit: against a real scratch space, switching to Anytype and restarting pushes a Bot's `memory/` and `skills/`; editing a file on disk shows in Anytype within about 10 s; editing the `hb_content` property in Anytype changes the file; Settings shows the state.

### M6. Live change feed (about 3 days)

- Enable `reqwest`'s `stream` feature. Implement `client/stream.rs`: open the search stream for `hb_file`, parse SSE (`object_added`, `object_updated`, `object_removed`, `snapshot_complete`), heartbeat, reconnect with backoff (a reconnect is a fresh snapshot, so diff it against the manifest; an object missing from the snapshot may only have left the set, so confirm with one read before acting).
- The engine treats a stream event as a hint to list and fetch that path now. Keep the 10 s poll as the fallback when the stream is refused (cap hit, 429) or down.
- Only the cheap fields are requested over the stream, never content.
- Echo guard: ignore an event whose `hb_device` equals this device and whose `hb_sha` equals the one just pushed.
- Exit: a remote edit is applied within a couple of seconds; killing the stream falls back to polling without a gap; a mock test drops the stream mid-run.

### M7. Hardening and cleanup (about 4 days)

- Tombstone garbage collection: archive tombstones older than 30 days (whichever device sees it first), tolerate `409 object_archived` races.
- Duplicate `hb_file` cleanup from 4.
- `hb_conflict` resolution detected from the Anytype side.
- Outbox cap, history ring pruning, manifest compaction.
- Logging: one debug line per decision (`path`, `decision`, shas), never content or keys. A small `sync.log` ring under `~/.hbots/sync/`.
- **Soak**: 10 Bots, 2,000 files, a day of simulated edits and random kills on the mock. Compare final trees.
- **Live smoke script** `tests/anytype-smoke.sh` (opt-in, needs `HBOTS_ANYTYPE_*` env): pair, bootstrap, push, edit both sides, conflict, resolve, delete. Not in CI.
- Update `docs/anytype-backend.md` status lines, `docs/backends.md` and `docs/PLAN.md` blurb; add the compatibility notes for `~/.hbots/sync/`.
- Exit: soak and live smoke clean, docs updated.

Rough total: about 7 to 8 weeks of focused work for M0 to M7. M3 and M4 carry most of the risk.

## 7. Test plan summary

| Layer | What | Where |
|---|---|---|
| Unit | policy rules, manifest, scan, decide (every row), merge, limiter, history, outbox, error mapping, schema bootstrap | next to each module in `tests/` folders |
| Integration vs mock | every action in 5.4 and 5.5, every failure in 5.8, first sync, conflicts, resolve | `storage/sync/tests/*.rs` |
| Fault injection | kill between disk write and push, push and ack, ack and manifest write, during a pull rename | hook trait in the engine |
| Concurrency | two engines, one mock, random edits, invariant "no acknowledged local write lost" | `sync/tests/concurrent.rs` |
| Contract | recorded `openapi.json` snapshot, diff on the paths and schemas the client uses | M1, CI uses the recorded copy |
| UI | Settings Sync card: locked fields, dry run, conflict actions | `app/app.test.contract` |
| Existing suites | `cargo test --locked`, `tests/e2e.sh`, `tests/compat/run.sh` unchanged in filesystem mode | every milestone |
| Live | `tests/anytype-smoke.sh` against a scratch space | manual, M5 and M7 |

## 8. Product and copy rules

- All Settings text goes through `app/strings/en.json` and `app/strings/fr.json` (keys under `settings.storage.*`, `settings.sync.*`). The host locale picks the table. Do not hardcode English in the contract. See `.agents/skills/exact-client/SKILL.md`.
- State names shown to the person: Syncing, Up to date, Offline (changes are kept and will sync), Needs your attention (a conflict), Not connected. No jargon like etag, outbox or tombstone in the UI.
- The Anytype option stays gated on a reachable space (already built). Enabling sync additionally requires the schema to be present (M2).
- Never show or log the key. Never put secrets in Anytype.

## 9. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Anytype API v2 is pre-release and may change | One client module, recorded `openapi.json` contract test, `api_changed` read-only state |
| Type and property creation is permanent | M0 in a scratch space, `dryRun` before bootstrap, additive schema only, confirm before touching the real space |
| Concurrent merge behavior across Anytype instances is unknown until P9 | The mock peer assumes the worst (torn writes), content authority, history ring, conflicts keep both |
| A harness turn edits a file while the engine pulls | Re-hash immediately before the atomic rename, abort on change, conflict next cycle |
| Large first sync is slow (write limit about 1/s) | Dry run with an estimate, 50 writes per cycle, resumable, status shows progress |
| JSON blobs (`library.json`) cannot be line merged | `Whole` policy: conflict on concurrent edits. Real fix is per-record objects in the typed-store phase |
| `.bot/attachments/*.json` plus `.bin` pairs may arrive half applied | Pull the `.bin` before its `.json`, push in the same order |
| A person edits a text property in Anytype and breaks formatting | Expected, it is content. The sha is recomputed, history keeps the previous version |
| Shared space exposes everything and lets another member write objects machines pull | Setup refuses a shared space unless the person overrides (already detected by the check route) |
| Clock differences between machines | No decision uses wall clocks across machines. Shas decide. `modified` is only a hint and a cursor |
| Disk scan cost on a big tree | mtime and size shortcut, allow list only, scan every 3 s, 200 ms CPU budget |

## 10. Differences from the design doc

1. The design puts `SyncedWorkspace` in the middle of every write. Only `Library` uses `Workspace`, so this plan uses a scanning engine and keeps `SyncedWorkspace` as an optional nudge (decision in section 2).
2. The design's `.bot/sync-manifest.json` becomes `~/.hbots/sync/manifest.json`, so a folder copy cannot carry stale state.
3. The design's file paths (`gateway/src/anytype/...`, `gateway/src/workspace.rs`) are stale. The code now uses `gateway/src/storage/adapters/anytype/` and `gateway/src/storage/workspace.rs`.
4. The design's P1 (trait hardening, `JsonDoc`, typed store traits) is **not** a prerequisite here. It stays valuable for later typed stores.
5. The design's P2 settings work (pairing, key, space name, check) is already shipped. What remains of P2 is the object and schema client (M1, M2).
6. `bot.json` is not synced in v1 (section 5.2). The design syncs it as `hb_bot` in P4.

## 11. Definition of done for v1

1. With Anytype chosen and the schema prepared, after a restart the engine runs and Settings shows its state.
2. A new file under a Bot's `memory/`, `skills/` or `library/` appears in Anytype as an `hb_file` object within about 10 s, with the exact bytes.
3. Editing that object's `hb_content` in Anytype updates the file on disk, unless the file changed locally in the meantime, in which case a conflict is raised and both versions are kept.
4. Deleting a file in either place deletes it in the other, with the old copy kept in the history ring.
5. Killing the gateway at any point, or stopping Anytype, loses nothing and duplicates nothing after recovery.
6. A conflict is visible in Settings and in Anytype, and resolving it in either place clears it.
7. Switching back to File system leaves the folder intact and the app working.
8. All CI checks pass. `tests/e2e.sh` and `tests/compat/run.sh` pass unchanged in filesystem mode. No file exceeds 500 lines.
9. No secret, token or key appears in Anytype, logs or API responses.

## 12. Notes for the implementing agent

- Work milestone by milestone. Open one PR per milestone. Do not start M3 until M0 results are written down.
- Do not add features beyond this file. If a decision is missing, choose the simpler option and note it in the PR.
- Before editing a file, read it fully. Match the existing style. Prefer small commits.
- Ask the person before anything that writes to a real Anytype space, creates types, or runs the live smoke script.
- Keep `AGENTS.md` and `.agents/skills/*` in sync if you change how the stack runs. Record spike results in `docs/anytype-spike.md`.
