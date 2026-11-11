---
name: dev-watch
description: Start or troubleshoot the hbots local development watch environment when asked to run the app or dev stack.
---

# Start the development stack

Read the repository's [DEV.md](../../../DEV.md) for current prerequisites, setup and troubleshooting. Run commands from the repository root.

1. Check whether the hbots dev stack is already running and usable before starting another copy. Check listeners on ports 8765 and 3780 and the T3 Code server on 3773. Do not stop or replace an unrelated process or another person's existing session. The watch script can stop an existing hbots listener on 3780 after building; account for that before starting it.
2. Confirm Bun, Cargo, `lsof`, the Exact checkout (`EXACT_DIR` or `.exact-path`), and the hbots/T3 configuration described in DEV.md. If setup is missing, follow that guide; do not print tokens or settings containing secrets. T3 Code must already be running; the watch script does not start it.
3. Start `bun scripts/dev.mjs` in a persistent terminal session. Use `--lan` only when the user requests network access, and consult DEV.md for the trusted-network exposure and `HBOTS_DEV_HOST`. The script assumes gateway port 3780, so check for conflicting `PORT` settings.
4. Check startup output and that the web dev server and gateway are responding. Report the actual web address (normally http://127.0.0.1:8765), any startup failures, and how to stop the session. Keep the watch process running for the user; do not claim it started if it exited.
