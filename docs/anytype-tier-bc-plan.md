# Tier B and Tier C Anytype sync implementation plan

Status: approved for implementation. The durable architecture and property catalog are in [anytype-backend.md](anytype-backend.md) and [anytype-schema.md](anytype-schema.md). Tier A is live; P4 typed stores precede P5 page-backed documents. The schema prefix is `m_*`.

## Invariants

- Typed objects own shared Bot, settings, room, schedule, connector and record data. Do not expand the Tier A file allow list to include mixed shared/local JSON.
- Machine bindings, runtime state, UI preferences, secrets and T3 task IDs remain local. Migrate existing files without losing data. Never sync storage configuration, vault data, keys or transcripts.
- A Bot has one home device. Only its home runs turns and schedules; handover drains and pushes before release and rebinding. Group members must share a home.
- Schema changes are additive. Version 2 requires re-Prepare and protection against older gateways writing to a newer schema. Do not enable typed writers before the prerequisites are in place.
- Filesystem mode remains compatible. Keep Rust files at most 500 lines and T3 RPC names inside the T3 adapter.

## Prerequisites

1. Split Bot local fields into `.bot/local.json` and group runtime into `.botapp/groups.local.json`; migrate on load and merge at all readers. Explicitly deny both paths in file sync.
2. Introduce Bot, settings, room, record, schedule, connector and local-state store interfaces with filesystem adapters. Preserve existing behavior.
3. Add all P4 types and properties from the schema catalog to idempotent Prepare, then bump schema metadata to v2. Check the minimum gateway version before writing. Verify with a scratch space.

## P4: typed stores

Build a cached object reconciler with stream and polling fallback, local write-through, typed outbox and idempotency based on device, type, identity, operation and content hash. Reuse import, conflict, heartbeat and status machinery.

1. Sync shared settings; keep theme and other UI preferences local.
2. Sync Bot definitions and materialize folders. Cache Bot listings, resolve slug collisions deterministically, and assign a home device without inventing remote T3 bindings.
3. Enforce home-only execution and implement drain/force handover. Test two simulated devices.
4. Sync model profiles and connector public fields. Resolve credentials locally and regenerate derived tool configuration.
5. Sync schedule definitions; reconcile T3 tasks only on the home device, with task IDs and review state local. Keep webhook secrets local.
6. Migrate library records into the eight typed record objects and attachments; retain filesystem format in filesystem mode.
7. Sync group metadata and append-only per-device message segments; merge messages by timestamp and ID, retaining runtime and unread state locally.
8. Extend import preview, verification, CLI and status with typed counts, freshness, pending operations and home Bot count.

## P5: page-backed documents

First implement and test a Markdown/Anyblock codec against real fixtures. Preserve unknown blocks in a sidecar, normalize idempotently, and follow the front-matter and block rules in the design record. No page writers until the codec and Bot/home rules are ready.

Then implement prompts and derived instructions; memory and shared memory; skills including the existing flat layout; and a measured decision on log granularity. Track materialized paths, object IDs and rendered hashes in a local manifest. Merge rendered Markdown against the last common base, preserve both sides on conflict, and defer memory application while a Bot is active. Migrate ownership away from `m_file` without double syncing. Import in dependency order: settings, Bots, catalog, records, prompts, memory, skills, files and attachments, groups.

## Verification and delivery

Use focused mapper, migration, collision, handover, segment and codec tests; mock Anytype stream, concurrent edit and failure tests; and the compatibility harness in its temporary workspace. Exercise Prepare and import against a scratch Anytype space and a copied home, then verify a second simulated device or independent gateway. Extend the soak test for typed metadata and messages. Never use a real user home as a test fixture.

P4 is complete only when all typed domains sync, local overlays and home enforcement work, import and verify cover typed objects, and filesystem compatibility passes. P5 is complete only when page documents round-trip, conflicts preserve both sides, migrated paths have one owner, and the harness still reads materialized files.

Open implementation decisions: log entry granularity; whether project boards merge into record types; and the exact shared webhook URL policy. Keep secrets local in every case.

## Implementation notes

Anytype-mode Bot and group local overlays are migrated on read. The local file is written before the shared file, so an interrupted split can be retried. Switching the backend back to filesystem restores recognized local fields into the legacy files. Filesystem homes that have never used overlays retain their existing format.

Anytype-mode gateways use cooperative file locks around Bot and group read-modify-write operations; the lock files remain active when returning to filesystem mode. Untouched filesystem-only homes keep their previous behavior. Restore journals protect retries from overwriting intervening local edits. The two-file migrations are still not transactions, and older gateways do not take these locks or understand the overlays. Stop older gateways using the same home before switching storage backends or migrating an existing Anytype home. Cross-version concurrent use and interrupted migration recovery across different gateway versions remain unverified; these locks do not provide distributed home-device enforcement.

Settings reads and writes now pass through a `SettingsStore`; Bot instruction rendering receives the same store. Its filesystem adapter retains the existing settings JSON behavior. A backend read error refuses settings updates and uses an approval-required runtime cap for read-only fallback. Library records now pass through per-record `RecordStore` operations, with a filesystem adapter that retains `library.json` and workspace nudges. The remaining four domain stores are in place: `BotStore` (profiles, the main-Bot choice, and the local overlay split/restore, which now lives with the filesystem adapter), `RoomStore` (the `groups.json` document, its `groups.local.json` sidecar and the groups file lock), `ScheduleStore` (the routine change history), and `LocalStateStore` (machine-local JSON documents, wired to thread-meta). In every case the filesystem adapter preserves the previous exact behavior; the shared-data and machine-local split is now confined to storage, not `bots/` or `chat/`. Anytype adapters for the four stores are outstanding.

The Anytype typed cache is staged and disconnected from the running engine. It hydrates type-scoped search results, treats stream events as invalidations, rejects incomplete pagination and stale in-flight results, and retains missing objects as uncertain. It cannot yet drive deletion or writes: stream scheduling, durable outbox, natural-key reconciliation and live typed-object validation remain necessary.
