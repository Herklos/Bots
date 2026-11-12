---
name: exact-client
description: Implement or debug hbots Exact client changes, including Contract, client sources and localized UI text.
---

# Exact client

Read `AGENTS.md` and `DEV.md` for the checkout and build setup. Exact is a separate checkout identified by `EXACT_DIR` or `.exact-path`; consult its Contract documentation when needed. The client is in `app/`. `app/app.contract` is the root (routes and `component Bots`); screens and members live beside it, and `app/README.md` records the layout. Data sources are in `app/app.ts`. A screen reads root values from the record `Bots` provides (`chatBag`, `settingsBag`, and the rest) and injects actions by name. Pass a prop when only that use needs the value.

Put new visible chrome in both `app/strings/en.json` and `app/strings/fr.json`; do not hardcode new English UI sentences. Preserve user-entered and gateway-provided content as described in `AGENTS.md`. Follow existing Contract patterns and update generated declarations when the build requires it.

For a release-style web build, load `.exact-path` if necessary and run `EXACT_APP_DIR="$PWD/app" bun "$EXACT_DIR/host/web/build.mjs" bots-web`. The watch loop instead writes `app/target/web-dev`; do not confuse it with `app/target/web-dist`. Run relevant Contract or integration checks when the change warrants them. For visual behavior, follow the available device or browser tooling instructions and report what was actually verified.
