# Anytype spike: pairing (S1), 2026-11-12

Live run against the desktop app (`127.0.0.1:31009`, API `2025-11-08`). One approval in the app.

- `POST /v2/auth/challenges` with `{"app_name":"hbots"}` returned a `challenge_id` and opened the approval dialog. No key needed, no `Anytype-Version` problem (header sent anyway).
- After approval and choosing spaces, the app showed a 4 digit code. `POST /v2/auth/api_keys` with `challenge_id` and `code` returned `api_key` (`anytype_<body>_<checksum>`) and a `grant` (`all_spaces`, `space_ids`, `permission`).
- `GET /v2/auth/whoami?ids=full`: `scope: jsonApi`, `key_status: scoped`, `grant.restricted: true`, `grant.spaces[]` with `id`, `name`, `permission`, `key.name: hbots`.
- `GET /v2/spaces?ids=full` lists only granted spaces and sets `has_not_granted_spaces: true` when others exist.
- `GET /v2/spaces/{id}/members` gives the member count, so a shared space can be detected (the plan refuses one by default). `Test-AnyBot` has two members.
- The key can reach several spaces if several are ticked in the dialog. Tick only the hbots space.

Gateway routes built on this: `POST /api/storage/anytype/pair`, `.../pair/confirm`, `.../check`. S2 to S8 are not run.

Settings connect flow (2026-11-12): run through the gateway routes against Anytype Desktop. `pair` opened the approval dialog, `pair/confirm` with the 4 digit code returned `keySet: true` and wrote `~/.hbots/anytype-key` (mode 600, never returned), and `check` returned `reachable: true`, `scoped: true`, `permission: readwrite`. The key reached one space, `Test-AnyBot`, not the default `hbots`, so `spaceFound` was false. Settings shows that as a missing space.
