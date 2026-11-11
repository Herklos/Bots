---
name: compatibility
description: Assess hbots persisted-data and shared-home compatibility when changing Bot, Vault, session or settings formats.
---

# Persisted-data compatibility

Identify the affected readers, writers and existing fixtures before changing a format. Preserve existing data and compatibility unless the user authorizes a migration; describe any versioning or migration needed. Never inspect or overwrite a real user home for a test.

Use `tests/compat/run.sh` in its default temporary workspace or another dedicated scratch directory. Read its options first if overriding `COMPAT_WORK`, ports or binaries. Add focused fixture coverage for meaningful format changes and run relevant unit tests. The harness tests two hbots processes against a shared home; check its output to see whether an earlier server is actually available before claiming cross-version or cross-implementation compatibility. Report what was tested and any unverified cases.
