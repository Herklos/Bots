# Gateway layout

`gateway/src` is split by domain. Each domain folder holds its logic, one file per concern. A backend's implementation lives under that domain's `adapters/` folder.

Domain code calls the harness. `gateway/src/harness/adapters/t3/` is the only place that names T3 RPCs. `scripts/check-t3-boundary.sh` fails when one of those names shows up anywhere else under `gateway/`.

`main.rs` is the entry point. `api/` is the HTTP handlers and the router, one file per area. Those handlers call the harness. They do not name T3 RPCs.

## Harness

`harness/` is the agent runtime. `Harness` in `mod.rs` groups the capabilities. Each one is its own trait:

| Trait | What the rest of the gateway asks for |
|---|---|
| Transcript | A thread read, inbox activity, and threads a restart would interrupt |
| Turns | A person's message, an interrupt, and a computer open or close |
| Forks | A side chat fork, the list on a parent thread, merge back, and delete if unused |
| Previews | The inbox preview for a thread |
| Routines | List, save, enable, delete, and run a routine |
| Approvals | An answer to a secret, approval, or question |
| Provisioning | Projects, a main thread, the brief, the model, and provider config |
| Browser | Pages, frames, take and control, input, tabs, and typing |
| Relay | The `/t3` HTTP and websocket proxy |
| Repos | Whether a workspace folder is a repository. The T3 adapter returns unsupported and sends no request |

`snapshot.rs` holds the neutral values those traits return. `error.rs` is unavailable, failed, or unsupported. `providers.rs` lists models for the computer the gateway is using. `connect` returns `Arc<dyn Harness>`.

`adapters/t3/` is the T3 Code client. `client.rs` is the transport. The other files match the capabilities: `transcript/`, `turns.rs`, `forks.rs`, `previews.rs`, `routines.rs`, `approvals.rs`, `provision.rs`, and `browser.rs` with `stream.rs`, `hold.rs`, and `fill.rs`. `setup.rs` is `hbots up` finding T3 Code and saving the token. `adapters/mock.rs` is the test double.

## Other domains

- `storage/`: `workspace.rs` is the trait the library and connectors use. `settings/` is the saved choice. `adapters/` has `filesystem/`, `anytype/`, and `memory`.
- `bots/`: Bot records, the roster, usage, the provisioner, and the computer screen. The computer tab picker stays here.
- `chat/`: transcripts the client sees, rooms, routines as the person named them, approvals as the person answered them, and the code gate.
- `projects/`: the old Projects board, still off.
- `vault/`: secrets and browser fill. Fill calls the harness.
- `network/`: sessions, exposure, and the tool server.
- `platform/`: config, the `hbots` command, release, and service install. `hbots up` calls the harness adapter.

A `.rs` file is at most 500 lines, tests included. `scripts/check-rs-length.sh` enforces it. An app contract or script is at most 300 lines. `scripts/check-size.mjs` enforces that. Do not raise either limit. Split the file.
