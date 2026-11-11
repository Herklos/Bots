---
name: native-builds
description: Build or troubleshoot hbots Exact native clients on macOS, iOS or Linux, including native build prerequisites and device validation.
---

# Native builds

Read `docs/native.md` and the relevant Exact checkout documentation; its recorded status is historical, so verify the current environment before relying on it. Locate Exact through `EXACT_DIR` or `.exact-path`. Native clients need an appropriate `HBOTS_URL` at build time, plus the platform toolchain and Hermes prerequisites described in the guide.

For macOS the documented build is `EXACT_APP_DIR="$PWD/app" bun "$EXACT_DIR/host/apple/build.mjs"`; `scripts/run-macos.sh` is the interim launcher. Select the requested platform and use the current Exact instructions for other targets. Distinguish a successful build from a launched and tested app, and a simulator build from a signed, installable distribution.

Use the session's mandatory device tooling and instructions for simulator or emulator interaction. Do not assume signing credentials, device availability, or that a desktop launcher owns the gateway. Report the artifact path, tests performed and any blockers.
