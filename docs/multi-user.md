# Multi-user (design)

Status: proposal, nothing built. Scope: a family or a few trusted people on one machine.

## Decisions

- **Trust model.** Everyone is trusted not to attack the machine. The gateway keeps people out of each other's Bots, Vault and settings. It is not a sandbox: all Bot threads still run as the one machine account under T3 Code.
- **Conversations.** Each Bot chooses `per-user` (each person has their own thread with it) or `everyone` (one thread all members share).
- **Sharing.** Each Bot and room is `private` (owner only) or `shared` (owner plus a named list of users). The owner can change this at any time.
- **Vault.** Per user. A Bot only ever uses its owner's logins, even when shared.

## Model

- `users.json` in the app dir: `id`, `name`, `password_hash` (argon2), `role` (`admin` or `member`), `created`.
- Bot and room metadata gain `owner`, `visibility` (`private` or `shared`), `members` (user ids) and, for Bots, `thread_mode`.
- Per-user settings (name, theme, default model, hidden or pinned Bots) move from the global settings into the user record.
- Session cookie becomes `v2.<user>.<exp>.<sig>`. The existing HMAC secret, lockout and same-origin check stay.
- Existing install migrates in place: one `admin` user is created from the current password and display name, and every existing Bot and room is `owner = admin`, `private`.

## Access rule

One function, `can(user, resource) -> none | use | own`, used by every route that takes a Bot slug, room id, thread id, attachment id or vault id.

- `own`: owner or admin. Can change sharing, rename, delete.
- `use`: a member of a shared resource. Can chat, read, react.
- `none`: answers 404, not 403, so private slugs are not revealed.

Lists (`/api/bots`, rooms, vault) are filtered through the same function. The 64 routes in `main.rs` are the audit surface, so a test table asserts every route against three users.

## Gateway changes

1. `users.rs`: store, argon2 hashing, create, change password, delete.
2. `auth.rs`: login takes a name and a password. The session carries the user id. An axum extractor gives handlers a `User`.
3. `access.rs`: the `can` function and the list filters.
4. Thread routing: for `per-user` Bots, derive the T3 thread from `(bot, user)`. The Bot's identity is the same, so the workspace and memory are shared by design.
5. Vault and Bot tools (`vault_tools.rs`, `mcp.rs`) resolve the vault by the thread's owner, not the caller.
6. Usage reporting stays per user, plus a household total for admins.
7. Admin routes: list, add, remove users and reset a password.

## App changes

- Sign-in takes a name and a password. With one user it keeps today's password-only screen.
- Roster shows what you can use. Shared Bots carry a small people glyph.
- Bot panel gets a Sharing page: private or shared, who, and the conversation mode.
- Rooms: a member picker limited to Bots you can use.
- Settings: your own name, theme, model and password. An admin gets a People section.

## Order of work

1. `users.rs`, session change, migration, and the `can` function with the route test table.
2. Filtering, per-user threads and the Vault split.
3. App: sign-in, roster filtering, Sharing page, People section.
4. Docs and `hbots` CLI for creating the first extra user.

Each step is one commit with tests passing.

## Open points

- A Bot's workspace files are shared between everyone who can use it. Say so on the Sharing page.
- A deleted user's private Bots go to the admin. Their Vault is deleted.
- `HBOTS_PASSWORD` stays as the admin password on a fresh install, so scripts keep working.
