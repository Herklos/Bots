# Native builds of the Exact app (T9)

Scope: ship `app/` (the Bots Exact client) on the web, Linux, macOS and iOS. Source of truth for commands and
keys: the Exact clone's `README.md` (quick start, section 3 "Run it natively"), `docs/start-here.md`, and
`scripts/app.schema.json` (`host.<platform>` keys). Those are cited by path below.

## Status by platform

| Platform | What was run | Result | Remaining |
|---|---|---|---|
| Web | Lead-reported, not run by this task: `contract build app/app.contract` and the web build (`EXACT_APP_DIR=<worktree>/app bun host/web/build.mjs bots-web`) and `tests/e2e.sh` per `docs/PLAN.md` (T7). | Not verified by this task. | Re-run to confirm on this branch. |
| Linux | `cargo build --profile host-dev -p caltrain-linux` in the Exact clone (sample app, to learn the toolchain). Hermes install for `x86_64-unknown-linux-gnu`. | Build: still compiling dependencies when the run ended; no result. Hermes: `cannot download pinned Hermes bundle https://github.com/expo/ibex/releases/download/hermes-vanilla-d412d3bd8512-v5/hermes-vanilla-x86_64-unknown-linux-gnu.tar.gz: io: invalid peer certificate: UnknownIssuer`, exit 1. | Linux native with app.ts: BLOCKED. The installer rejects the sandbox proxy CA. No Bots linux crate exists yet. `"linux": {}` added to `app/app.json`. |
| macOS | Not run. Needs Xcode on a Mac: `bun host/apple/build.mjs --run` (Exact README, section 3). | Not run: no macOS here. | Build and run on a Mac. `app.json` `host.macos` already has `minimumOS` and window size. |
| iOS | Not run. `bun host/apple/build.mjs --ios --run` (Simulator) or `--device --run`. | Not run: no Xcode here. | Needs a booted Simulator or device. `host.ios` has `minimumOS 17.0`. App Transport Security (below) must be handled for `http:`. |

## Requirements (from the Exact docs)

- Rust (rustup), Bun 1.4.2, and `bun scripts/exact.mjs setup` once (Exact README, "Install the tools").
- Hermes for TypeScript apps on native hosts (Bots has `app.ts`): `cargo run --manifest-path vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml -- --target <triple>`. This is the step that fails here.
- Linux: `cargo build --profile host-dev -p <app>-linux`. Headless is selected by `EXACT_DISPLAY=headless`, and `EXACT_SIZE=WxH` sets the viewport (`host/linux/src/app.rs`).
- macOS and iOS: Xcode. The first native build takes 10 to 15 minutes.

## app.json keys

`host` in `app/app.json` now has `ios`, `macos`, `linux` and `web`. Schema: `scripts/app.schema.json`. `host.linux` takes an
optional `painter` (only `"ink"`); unset, the GPU painter is used when available, else the CPU.

## Desktop shell (design, not built)

- A desktop shell starts `bots serve` as a child process and points the Exact app at it.
- The app reads its gateway URL from the `BOTS_URL` build-time environment variable (`env` in `app/app.json`).
  Build-time, not run-time: a per-launch URL needs a different mechanism, which this design does not cover.
- The shell owns the child: start it, wait for `/api/health`, stop it on quit.

## Phone access (design, not built)

- Tailscale: the phone joins the tailnet and reaches the gateway's Tailscale listener (T4 `exposure.rs`).
- A tunnel (for example `cloudflared`, T4 `remote_access.rs`) for access without Tailscale.
- App Transport Security: an `http:` gateway beyond the local network is refused by iOS unless `host.ios` sets
  `allowsArbitraryLoads` (`docs/reference.md`, around line 959). Prefer https through Tailscale or a tunnel.
- Session storage: the session cookie should live in `secret.keep` (grant `secret.keep`; `docs/recipes/files-and-grants.md`,
  line 10), not in `store`.

## Not done

- No Linux or Bots native host crate under `app/linux/`.
- No desktop shell code. Design only.
- No commit verified in this task; see the final report.
