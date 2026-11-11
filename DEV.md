# Development

The development stack runs the Exact web client and the Rust hbots gateway alongside a running T3 Code instance. The client is an Exact app, not an Expo web app.

## Prerequisites

- Install Rust and Cargo, Bun, and `lsof`.
- Start the T3 Code desktop app. Its server normally listens on `127.0.0.1:3773`; opening the app also installs the `~/.t3/bin/t3` CLI shim.
- Check out [Exact](https://github.com/expo/exact) separately. Set `EXACT_DIR` to its absolute path or create the gitignored `.exact-path` in this repository:

  ```sh
  EXACT_DIR=/absolute/path/to/exact
  EXACT_DIR_REL=../../contrib/exact
  ```

  `scripts/dev.mjs` reads `EXACT_DIR` from the environment first, then from `.exact-path`. `EXACT_DIR_REL` is useful for other local workflows; the dev script uses `EXACT_DIR`.
- Configure hbots and its T3 token. For a first local setup, build the binary and run:

  ```sh
  cargo build --release
  PATH="$HOME/.t3/bin:$PATH" ./target/release/hbots up --no-service
  ```

  Settings normally live in `~/.hbots/.env` and the token in `~/.hbots/t3-token`. The real environment takes precedence over `.env` files. See the [README](README.md#commands) for gateway settings.

## Watch the app and gateway

From the repository root, with T3 Code running:

```sh
bun scripts/dev.mjs
```

Open <http://127.0.0.1:8765>. The script starts:

- The Exact web dev loop on port 8765. It rebuilds the client and reloads the page when its sources change. Its output is `app/target/web-dev`.
- A debug gateway on port 3780. Changes under `gateway/src`, `design/src`, their Cargo manifests, or the workspace `Cargo.toml` trigger a rebuild and restart. If a build fails, the existing gateway is left running.

The client is built with `HBOTS_URL=http://127.0.0.1:3780` and calls the gateway directly. Web dev output is separate from the release build in `app/target/web-dist`. T3 Code is not started or stopped by this script.

Port 8765 must be free. Once a new debug gateway has built, the script stops an existing listener on 3780 whose command contains `hbots`; it refuses to stop a different program. Stop the dev stack with Ctrl-C. If a service manager restarts another gateway, stop that service before starting the dev stack.

The gateway still reads its normal settings, including `HOST` and `PORT`. The script assumes gateway port 3780; overriding `PORT` will make the generated client URL and the actual listener disagree. Normally leave `HOST` at its default loopback value for local development.

## Access from another device

```sh
bun scripts/dev.mjs --lan
```

This passes `--lan` to Exact and starts the debug gateway with `HOST=0.0.0.0`. Open `http://<any address of this machine>:8765` from a device that can reach it: the LAN address, the Tailscale address, `<hostname>.local`, or any other interface. One run answers on all of them.

In LAN mode a small proxy listens on 8765 and the Exact loop moves to 8766. The client is built with the gateway placeholder, as in a release, and the proxy replaces it with `http://<host the browser used>:3780`, so each device calls the gateway on the address it already reached. Exact is started with `--allow-host` for every IPv4 address of this machine plus `<hostname>` and `<hostname>.local`; a different name (for example a MagicDNS name) is refused by Exact's host check. Ports 8765, 8766 and 3780 must all be free and reachable through the host firewall. `HBOTS_DEV_HOST` only chooses the example address printed at startup. Devices normally need to be on the same network or tailnet; VPNs, firewalls, and Wi-Fi client isolation can prevent access.

**Use LAN mode only on a trusted network.** Both development services are exposed to network peers. The debug gateway permits requests from any browser origin by reflecting the request origin for credentialed CORS and bypassing its origin check. This does not disable gateway authentication, but if no password is configured the gateway is open. Traffic here uses HTTP. The dev script enables this behavior with `HBOTS_DEV_ALLOW_ANY_ORIGIN=1`; release builds ignore that setting and retain their normal origin checks. The Exact dev server may also expose development output and errors to peers.

That development switch is separate from Settings → Access → Network. The setting offers this device, the local network, and Tailscale Serve. Local network and Tailscale require a password, keep the normal origin checks, and apply when the gateway restarts. `HBOTS_NETWORK_ACCESS=local`, `lan`, or `tailscale` overrides the setting and locks it. `bun scripts/dev.mjs --lan` sets `HOST=0.0.0.0` for that process, which locks the control the same way a legacy `HOST` does, and the debug origin switch still allows it without a password. A release build does not.

## One-shot build and native client

To build the web assets served by the gateway outside the watch loop:

```sh
set -a; source .exact-path; set +a
EXACT_APP_DIR="$PWD/app" bun "$EXACT_DIR/host/web/build.mjs" bots-web
./target/release/hbots serve
```

Rebuild the web app after client or Contract changes. Release archives bundle the web assets alongside the binary. For macOS and other native builds, including the interim `scripts/run-macos.sh` launcher, see [docs/native.md](docs/native.md).

## Tests and troubleshooting

```sh
cargo test
cargo fmt --all -- --check
tests/e2e.sh
```

The end-to-end script needs Bun, Cargo, Playwright and an existing `app/target/web-dist` build. It runs a mock T3 and a gateway on separate ports; its output directory contains logs and screenshots.

- **Exact checkout not found:** set `EXACT_DIR` or check `.exact-path` and verify `host/web/dev.mjs` exists there.
- **Port 8765 in use:** stop the other web dev loop. The wrapper uses fixed ports and does not forward an alternate port option.
- **Port 3780 in use:** stop the other service if it is not an hbots process, or if it is automatically restarted.
- **Gateway cannot reach T3:** check that the desktop app is running, the token is configured, and `T3_URL` points to the T3 server.
- **Another device cannot connect:** check the printed address, set `HBOTS_DEV_HOST` if needed, and check access to both ports and the host firewall. `127.0.0.1` on the other device refers to that device, not the development machine.
