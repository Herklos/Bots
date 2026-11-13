# Result table

| # | Question | One line result |
|---|---|---|
| 1 | Request shapes | Record type/create, property, object create (shortcut body with `properties` plus `markdown`), patch through `ops` only (`set_properties`, `insert_blocks`, `replace_text`), search with query-string pagination, DELETE to the Bin, `restore`, as written in S2 |
| 2 | Property limits | 256 KiB round-trips byte exact in 0.04 s; byte exact through 4 MiB; the real cap is the 10 MiB request body (413 `request_too_large`), not a property limit |
| 3 | Files | `POST /v2/spaces/{id}/files`, multipart field `file`; name keeps the base without extension, mime sniffs (`text/plain`); the property stores an array of file ids; ranged download 206; 1 GiB upload accepted |
| 4 | Etags | Create and patch responses carry `etag` in body and `ETag` header; stale `If-Match` is 409 `etag_mismatch` with the current etag in the message; patch without `If-Match` is allowed |
| 5 | Idempotency-Key | Replay returns the same object id and same body; same key with a different body is 409 `idempotency_conflict`; replays work across process restarts |
| 6 | Rate limiting | Burst of 100 creates: 60 accepted, 40 got 429 `rate_limit_exceeded`, no `Retry-After` header anywhere; sustained rate measured at 1 write per second |
| 7 | Latency | p50/p95 ms: get 2.7/7.3, patch 6.2/7.3, search of 100 rows 9.3/19.7, 256 KiB upload 1.4/2.4 |
| 8 | Change detection | A body edit in the app sets `last_modified_date` and `last_modified_by`; the field is absent until first modification; `last_modified_date` filters only work with day precision, timestamp or epoch filters match nothing, so poll by sort plus client-side compare |
| 9 | Search stream | Works on this build; SSE with per-match `object_added`, `snapshot_complete`, then `object_added`, `object_updated`, `object_removed`, `: keepalive` heartbeat; the app quitting closes the stream cleanly (EOF), no error event |
| 10 | Key normalization | Explicit keys without digits come back unchanged (`m_explicit_key`, type `m_spike_file`); `m_probe2` comes back stored as `m_probe_2` |
| 11 | Archive and tombstone | Archived object leaves search, GET stays 200, PATCH is 409 `object_archived`; `restore` works; `?permanent=true` deletes for good; a tombstone object stays searchable and editable |

All runs live against Anytype Desktop on `127.0.0.1:31009`, API version `2025-11-08`, in the scratch space billed `Bots` (the renamed `Test-AnyBot`, confirmed by the person before any write). One throwaway type (`m_spike_file`) and its `spike_*` properties stay in that space forever, as the design predicts. No secrets written.

## S2. Request shapes (from `GET /v2/docs/openapi.json`, verified live)

- Create type: `POST /v2/spaces/{id}/types` with `{"name": "...", "api_key": "m_spike_file", "layout": "basic", "property_definitions": [{"name": "...", "format": "text|number|checkbox|files..."}]}`. `api_key` is the explicit key. Response 201: `{"id", "key", "etag", "created": {"properties": [{"key", "name", "format"}...]}}`.
- Create property: `POST /v2/spaces/{id}/properties` with `{"name": "...", "format": "text", "key": "m_explicit_key"}`. Response 201: `{"id", "key"}`. Unknown names inside `property_definitions` mint lowercase snake keys (`Spike Path` becomes `spike_path`).
- Create object: `POST /v2/spaces/{id}/objects`, shortcut body `{"type": "m_spike_file", "name": "ada/memory/MEMORY.md", "properties": {"spike_path": "...", "spike_size": 12}, "markdown": "# H\n\nbody"}`. The second arm of the spec is a full AnyBlock document (`blocks` array). Response 201: `{"id", "type", "etag"}` both as `etag` member and `ETag` header.
- Read object: `GET /v2/spaces/{id}/objects/{oid}` returns the whole AnyBlock document: `properties` dict plus `blocks` list. `ETag` header present.
- Patch object: `PATCH /v2/spaces/{id}/objects/{oid}` with `{"ops": [{...}]}` only, 1 to 512 ops. No other body members exist. Verified ops: `{"op": "set_properties", "set": {"k": v}, "add": {...}, "remove": {...}, "unset": ["k"]}`, `{"op": "insert_blocks", "blocks": [{"type": "paragraph", "text": "..."}]}` (returns `created_blocks` map to real ids), `{"op": "replace_text", "find": "...", "replace": "..."}` (finds markdown source, 404 when absent). Request header `If-Match: <etag>`. Query: `dry_run`, `ids`, `create_missing_options`.
- Search: `POST /v2/spaces/{id}/search?limit=25&offset=0`; pagination lives in the query string, a `limit` body member is a 400 with hint `resend with ?limit=25`. Body: `filters` (structured nodes: `{"property", "condition", "value"}` or `{"operator": "and|or", "filters": []}`) or a compact `filter` string, at most one of the two; also `type`, `fields`, `sorts` (`{"property", "direction"}`, 10 max), `query`. Response: `{"data": [{"id", "name", "type", "properties": {...}}], "total", "offset", "limit", "has_more"}`.
- Archive: `DELETE /v2/spaces/{id}/objects/{oid}` (Bin), `?permanent=true` deletes for good, `?dry_run=true` probes. Response 200: `{"id", "type", "warnings": [{"path": "object_id", "message": "moved to the Bin", "hint": "restore it with POST ..."}]}`.
- Restore: `POST /v2/spaces/{id}/objects/{oid}/restore`. Response 200: `{"id", "type"}`.

## S3. Property limits

Patch loop on one text property with multibyte bytes:

- 256 KiB: exact round-trip, write 0.04 s. 400 KiB: 0.07 s. 512 KiB: 0.08 s. 1 MiB: 0.15 s. 2 MiB: 0.27 s. 4 MiB: byte exact.
- 8 MiB: 413 `request_too_large`, message `request body exceeds the 10 MiB limit` (the JSON escapes inflate the payload).
- A 16 MiB body reset the connection mid-upload (server never answered).

Measured fact: 256 KiB is safe and fast, the design's 1 MiB string limit is smaller than the real one, and any write is capped by the 10 MiB request body. Michel can rely on 1 MiB with room to spare.

## S4. Files

- Upload: `POST /v2/spaces/{id}/files`, `multipart/form-data`, one part named `file`, filename and Content-Type taken from the part. A JSON arm with `url` exists for import by URL. Response 201: `{"id", "name", "mime_type", "size"}`.
- Stored name: `notes.bin` became `notes`, `memory.md` became `memory` (the extension is dropped, Anytype keeps mime separately). `Content-Type` sent as `text/markdown` came back `mime_type: "text/plain; charset=utf-8"`; `application/octet-stream` survived as is; uniform ASCII uploaded as a `.bin` also sniffed to `text/plain; charset=utf-8`. So the mime mainly comes from Anytype's sniffer.
- `m_blob` reference: a `files` format property holds a plain array of file id strings in the object document and in search rows: `"spike_blob": ["bafyrei..."]`.
- Download: `GET /v2/spaces/{id}/files/{fid}/content`, body byte exact, `Content-Length`, `ETag` (sha256-shaped), `Accept-Ranges: bytes`, `Last-Modified` absent. `HEAD` works. `Range: bytes=256-511` returns 206 with the exact slice.
- Size limit: 100 MiB accepted in 0.7 s, 1 GiB accepted in 6.5 s, no limit reached. The 10 MiB JSON body cap does not apply to multipart.

## S5. Etags

- Create returns `/etag` body member plus the `ETag` response header. GET returns the `ETag` response header. Every patch response carries the new etag, body member and header.
- Stale `If-Match`: 409 `{"code": "etag_mismatch", "message": "... current etag is \"2a74aec0\"; re-read the object and retry"}`.
- PATCH without `If-Match` is accepted (200). If-Match is therefore the sync engine's discipline, not the server's.

## S6. Idempotency-Key

Header `Idempotency-Key` on create: replay with the same key and same body returns 201 with the same object id, same etag, identical body. Same key with a different body: 409 `idempotency_conflict`. Replay works from a fresh process with a new TCP connection (all calls in this spike are process fresh anyway). Same behavior on patch: replay returns the stored first response (same etag, first diff_stats).

## S7. Rate limiting

100 fresh-key creates as fast as possible (0.6 s wall):

- Status counts: 60 x 201, 40 x 429.
- 429 body: `{"object": "error", "status": 429, "code": "rate_limit_exceeded", "message": "You have reached maximum request limit."}`.
- No `Retry-After` header on any 429 (checked both cases and casings).
- Sustained: after the burst drains, token bucket refills to exactly 1 write per second (measured 1 success per second in a 10 s loop, 21 x 201 then 1 per second). File uploads did not get throttled in a 30 upload burst, so the bucket appears to cover mutating object routes, not the files route.
- The plan's limiter default (0.8 writes/s, burst 40) is safely inside the measured envelope.

## S8. Latency (localhost, seconds unless noted)

| Operation | p50 (ms) | p95 (ms) |
|---|---|---|
| GET one object | 2.7 | 7.3 |
| PATCH one property (with a separate GET for the etag) | 6.2 | 7.3 |
| Search returning 100 rows with fields projection | 9.3 | 19.7 |
| Upload 256 KiB file | 1.4 | 2.4 |

Everything is single-digit to tens of milliseconds. The 10 s remote poll at 100 page size leaves large headroom.

## S9. Change detection (poll cursors)

- A body paragraph edited by a person in the Anytype app updated `last_modified_date` (created 16:52:18Z, last_modified 16:53:04Z) and set `last_modified_by` to the participant id, while `created_date` stayed. The body text came back over `GET` through the blocks array.
- The requested property edit (Spike Path to `mzero/edit-me-person`) did not come through: after the person confirmed, `spike_path` still read `mzero/edit-me`. Presumed an in-app edit that lost focus before commit. Property writes through the API do set `last_modified_date` (patched objects all carry it), so the detection logic is unchanged.
- `last_modified_date` is absent on objects never modified after create: 89 of 94 objects in the burst had none until first patch. Treat it as "modified since creation", not "creation time".
- Filtering: the structured filter takes unix seconds for dates (`{"property": "last_modified_date", "condition": "greater", "value": 1794502324}`), the compact filter takes an RFC 3339 date, but on this build any timestamp or epoch value matched zero rows; only day precision works (`filter: "last_modified_date >= \"2026-11-12\""` returned the 5 rows that had the field).
- Conclusion for the poll cursor: do not filter on `last_modified_date` at second precision. Poll with the `fields` projection, sort by `last_modified_date` descending, compare client-side.

## S10. Key normalization

- Type: created with `api_key: "m_spike_file"`, read back `key: "m_spike_file"` unchanged.
- Property: created with `key: "m_explicit_key"`, read back unchanged.
- Property created with `key: "m_probe2"`: stored and returned as `m_probe_2`, exactly the design's `hbprobe2` -> `hbprobe_2` rule. Digits split the snake key, so Michel keys already avoid digits, which stays mandatory.
- Unkeyed properties mint lowercase snake keys from display names (`Spike Path` -> `spike_path`).

## S11. Archive and tombstone (audit A3 confirmed once)

- Archive (`DELETE`, no query): 200 with a "moved to the Bin" warning. Search for its exact path then returned total 0. `GET` still 200. `PATCH` is 409 `object_archived` ("in the Bin and cannot be edited until it is restored").
- `restore`: 200, the object is back in search (total 1) and editable.
- `?permanent=true` on an object this key created: 200, gone for good.
- Tombstone: a separate object with `spike_deleted: true` and `spike_content: ""` but not archived stayed in search (total 1, both fields visible through `fields`) and accepted further patches (200). The design's tombstone delete is therefore sound.

# Anytype spike: pairing (S1), 2026-11-12

Live run against the desktop app (`127.0.0.1:31009`, API `2025-11-08`). One approval in the app.

- `POST /v2/auth/challenges` with `{"app_name":"hbots"}` returned a `challenge_id` and opened the approval dialog. No key needed, no `Anytype-Version` problem (header sent anyway).
- After approval and choosing spaces, the app showed a 4 digit code. `POST /v2/auth/api_keys` with `challenge_id` and `code` returned `api_key` (`anytype_<body>_<checksum>`) and a `grant` (`all_spaces`, `space_ids`, `permission`).
- `GET /v2/auth/whoami?ids=full`: `scope: jsonApi`, `key_status: scoped`, `grant.restricted: true`, `grant.spaces[]` with `id`, `name`, `permission`, `key.name: hbots`.
- `GET /v2/spaces?ids=full` lists only granted spaces and sets `has_not_granted_spaces: true` when others exist.
- `GET /v2/spaces/{id}/members` gives the member count, so a shared space can be detected (the plan refuses one by default). `Test-AnyBot` had two members.
- The key can reach several spaces if several are ticked in the dialog. Tick only the Michel space.

Gateway routes built on this: `POST /api/storage/anytype/pair`, `.../pair/confirm`, `.../check`. S2 to S11 above are the M0 spike (run 2026-11-12 in the scratch space).

Settings connect flow (2026-11-12): run through the gateway routes against Anytype Desktop. `pair` opened the approval dialog, `pair/confirm` with the 4 digit code returned `keySet: true` and wrote `~/.hbots/anytype-key` (mode 600, never returned), and `check` returned `reachable: true`, `scoped: true`, `permission: readwrite`. The key reached one space, `Test-AnyBot`, not the default `hbots`, so `spaceFound` was false. Settings shows that as a missing space.

M0 addendum (2026-11-12): the scratch space `Test-AnyBot` was renamed to `Bots`; confirmed with the person before the S2 to S11 writes. It now has 1 member and only built-in types before the spike, but it is still the throwaway space, not the real Michel space.
