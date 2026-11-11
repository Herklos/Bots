---
name: hbots-release
description: Prepare hbots changelogs and releases when asked to summarize changes, bump a version, or tag a release.
---

# hbots releases

Find the latest published release and compare its tag with the intended release commit. Read the commits and relevant diffs, including changes that were not individually mentioned in commit subjects. Describe shipped behavior accurately; distinguish plans and documentation from implemented features.

Add a new entry at the top of `CHANGELOG.md`. Begin with a short, plain-language summary of what the release means to users. Follow it with sections such as **New features**, **Improvements**, **Bug fixes**, **Deprecations**, and **Breaking changes**, as applicable. Omit empty sections. State migration or replacement guidance for deprecations and breaking changes when known. Group related changes and focus on user impact instead of reproducing the commit log; mention development or release infrastructure only when useful to the audience. Never invent a change or claim an unverified fix.

When a version bump is requested, choose the version consistent with the changes and repository conventions, update the workspace version and lockfile, and run appropriate checks. Confirm the tag does not already exist and points to the intended commit. Commit, push, tag, publish, or monitor CI only as authorized by the user's request and conversation. A pushed `v*` tag starts `.github/workflows/release.yml`; inspect its result when monitoring is requested. Do not move an existing published tag without explicit authorization. Report the release URL or CI status accurately, including failures and incomplete checks.
