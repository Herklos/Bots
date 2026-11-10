# Native builds of the Exact app (T9)

Scope: ship `app/` (the Bots Exact client) on the web, Linux, macOS and iOS. Source of truth for commands and
keys: the Exact clone's `README.md` (quick start, section 3 "Run it natively"), `docs/start-here.md`, and
`scripts/app.schema.json` (`host.<platform>` keys). Those are cited by path below.

## Status by platform

| Platform | What was run | Result | Remaining |
|---|---|---|---|
| Web | 2026-11-10 on this Mac: Exact from `.exact-path`, `EXACT_APP_DIR=$PWD/app bun "$EXACT_DIR/host/web/build.mjs" bots-web` → `app/target/web-dist`; gateway serves it on :3780. Face screenshots via `tests/face/screenshot.sh` + system Chrome. | OK | Keep rebuilding after Contract changes. |
| Linux | Earlier: Hermes install for `x86_64-unknown-linux-gnu` failed (`UnknownIssuer`). No Bots linux crate under `app/linux/`. | BLOCKED here | Needs a Linux host with Hermes; `"linux": {}` is in `app/app.json`. |
| macOS | Xcode present (`/Applications/Xcode.app`). Command: `EXACT_APP_DIR=<hbots>/app bun "$EXACT_DIR/host/apple/build.mjs" --run`. Full native build not finished in this handoff (long first compile). | Tools ready; build not verified | Run the apple host build; `host.macos` has `minimumOS` 14.0 and window size. |
| iOS | Same apple host script with `--ios --run` (Simulator) or `--device --run`. | Not run | Needs Simulator/device. `host.ios` `minimumOS` 17.0; ATS for `http:` as below. |

## Requirements (from the Exact docs)

- Rust (rustup), Bun 1.4.2, and `bun scripts/exact.mjs setup` once (Exact README, "Install the tools").
- Hermes for TypeScript apps on native hosts (Bots has `app.ts`): `cargo run --manifest-path vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml -- --target <triple>`. This is the step that fails here.
- Linux: `cargo build --profile host-dev -p <app>-linux`. Headless is selected by `EXACT_DISPLAY=headless`, and `EXACT_SIZE=WxH` sets the viewport (`host/linux/src/app.rs`).
- macOS and iOS: Xcode. The first native build takes 10 to 15 minutes.

## app.json keys

`host` in `app/app.json` now has `ios`, `macos`, `linux` and `web`. Schema: `scripts/app.schema.json`. `host.linux` takes an
optional `painter` (only `"ink"`); unset, the GPU painter is used when available, else the CPU.

## Desktop shell (design, not built)

- A desktop shell starts `hbots serve` as a child process and points the Exact app at it.
- The app reads its gateway URL from the `HBOTS_URL` build-time environment variable (`env` in `app/app.json`).
  Build-time, not run-time: a per-launch URL needs a different mechanism, which this design does not cover.
- The shell owns the child: start it, wait for `/api/health`, stop it on quit.

## Phone access (design, not built)

- Tailscale: the phone joins the tailnet and reaches the gateway's Tailscale listener (planned in T4, `exposure.rs`, not merged yet).
- A tunnel (for example `cloudflared`; planned in T4, `remote_access.rs`, not merged yet) for access without Tailscale.
- App Transport Security: an `http:` gateway beyond the local network is refused by iOS unless `host.ios` sets
  `allowsArbitraryLoads` (`docs/reference.md`, around line 959). Prefer https through Tailscale or a tunnel.
- Session storage: the app keeps the session token with `store.set` under the `secret.keep session` grant (Keychain on Apple,
  a 0600 file on Linux, `localStorage` on the web; Exact `docs/reference.md`, "Store"). Native apps send it as a bearer token.

## Not done

- No Linux or Bots native host crate under `app/linux/`.
- No desktop shell code. Design only.
