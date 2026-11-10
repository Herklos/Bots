# Native builds of the Exact app (T9)

Scope: ship `app/` (the Bots Exact client) on the web, Linux, macOS and iOS. Source of truth for commands and
keys: the Exact clone's `README.md` (quick start, section 3 "Run it natively"), `docs/start-here.md`, and
`scripts/app.schema.json` (`host.<platform>` keys). Those are cited by path below.

## Status by platform

| Platform | What was run | Result | Remaining |
|---|---|---|---|
| Web | 2026-11-10 on this Mac: Exact from `.exact-path`, `EXACT_APP_DIR=$PWD/app bun "$EXACT_DIR/host/web/build.mjs" bots-web` → `app/target/web-dist`; gateway serves it on :3780. Face screenshots via `tests/face/screenshot.sh` + system Chrome. | OK | Keep rebuilding after Contract changes. |
| Linux | Earlier: Hermes install for `x86_64-unknown-linux-gnu` failed (`UnknownIssuer`). No Bots linux crate under `app/linux/`. | BLOCKED here | Needs a Linux host with Hermes; `"linux": {}` is in `app/app.json`. |
| macOS | 2026-11-10: `EXACT_APP_DIR=<hbots>/app bun "$EXACT_DIR/host/apple/build.mjs"` (no `--run`). Finished `host-dev`; wrote `app/target/clients/…/macos/hbots.app` (arm64). Note: no Metal toolchain → Canvas 2D uses Core Graphics (`xcodebuild -downloadComponent MetalToolchain`). | OK (build) | Launch with `scripts/run-macos.sh` (starts the gateway, opens the `.app`). The in-app shell that starts `hbots serve` is not built. |
| iOS | Same apple host script with `--ios --run` (Simulator) or `--device --run`. | Not run | Needs Simulator/device. `host.ios` `minimumOS` 17.0; ATS for `http:` as below. |

## Requirements (from the Exact docs)

- Rust (rustup), Bun 1.4.2, and `bun scripts/exact.mjs setup` once (Exact README, "Install the tools").
- Hermes for TypeScript apps on native hosts (Bots has `app.ts`): `cargo run --manifest-path vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml -- --target <triple>`. This is the step that fails here.
- Linux: `cargo build --profile host-dev -p <app>-linux`. Headless is selected by `EXACT_DISPLAY=headless`, and `EXACT_SIZE=WxH` sets the viewport (`host/linux/src/app.rs`).
- macOS and iOS: Xcode. The first native build takes 10 to 15 minutes.

## app.json keys

`host` in `app/app.json` now has `ios`, `macos`, `linux` and `web`. Schema: `scripts/app.schema.json`. `host.linux` takes an
optional `painter` (only `"ink"`); unset, the GPU painter is used when available, else the CPU.

## Desktop shell

Built: a dev script, `scripts/run-macos.sh`. Not built: the in-app shell.

**Built (interim entry).** The script:

- starts `target/release/hbots serve` if `GET $HBOTS_URL/api/health` (default `http://127.0.0.1:3780`) does not answer; it builds the binary first with `cargo build --release --bin hbots` if it is missing. Output goes to `target/serve.log`;
- waits up to 60 s for `/api/health`;
- opens the newest `app/target/clients/*/*/macos/hbots.app` if one exists. Opening leaves the gateway running, and the bundle keeps the `HBOTS_URL` it was built with;
- otherwise runs the Exact apple host with `--run` (`EXACT_DIR` from `.exact-path`, `HBOTS_URL` passed to the build). It stops the gateway it started when the app quits.

**Not built (the in-app shell).**

- The Exact host, in the `.app`, does not start or stop the gateway. The script is the only starter.
- `HBOTS_URL` is inlined at build (`app/app.ts`, `env` in `app/app.json`), so one bundle serves one gateway. A per-launch URL needs another mechanism, which is not chosen.
- Candidate routes, undecided: Exact's `willQuit(done:)` native-module hook, which runs before quit and is where a child is stopped (`docs/reference.md`, around line 336); and `@exact/child-process` (`docs/reference.md`, "Child processes"). The second is for app data code under grant lines (`process.spawn`, origin `"app"`), not a host API, so it may not fit a host-owned child. Neither has been tried.
- The shell must still wait for `/api/health` before it shows the app, and stop the child on quit.

## Phone access (design, not built)

- Tailscale: the phone joins the tailnet and reaches the gateway's Tailscale listener (planned in T4, `exposure.rs`, not merged yet).
- A tunnel (for example `cloudflared`; planned in T4, `remote_access.rs`, not merged yet) for access without Tailscale.
- App Transport Security: an `http:` gateway beyond the local network is refused by iOS unless `host.ios` sets
  `allowsArbitraryLoads` (`docs/reference.md`, around line 959). Prefer https through Tailscale or a tunnel.
- Session storage: the app keeps the session token with `store.set` under the `secret.keep session` grant (Keychain on Apple,
  a 0600 file on Linux, `localStorage` on the web; Exact `docs/reference.md`, "Store"). Native apps send it as a bearer token.

## Not done

- No Linux or Bots native host crate under `app/linux/`.
- No in-app desktop shell. `scripts/run-macos.sh` is the interim entry.
