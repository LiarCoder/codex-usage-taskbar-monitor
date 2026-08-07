---
name: release-app-version
description: Upgrades this Rust application's semantic version, completes its GitHub release pipeline through asset verification, and cleans stale branches after a successful release. Use when the user asks to “升级应用版本”, “发布新版本”, bump or release the app version, merge a release PR, create a v-prefixed tag, or trigger the release workflow.
---

# Release App Version

Use this project-specific workflow to publish a version safely and completely.

## Version confirmation gate

1. Read the current version from `Cargo.toml`, the latest `v*` tag, recent `Bump version` commits, and the changes being released.
2. If the user did not provide an exact target version in the current request, recommend a SemVer target and ask the user to confirm it.
3. Do not edit `Cargo.toml`, `Cargo.lock`, tags, releases, or PR state until the exact target version is confirmed.
4. If the user already supplied an exact version, treat it as confirmed unless it conflicts with an existing tag or release.

## Preflight

1. Confirm `gh` is installed and authenticated.
2. Fetch and prune `origin`; identify the default branch, current branch, associated PR, and release workflow trigger.
3. Stop for unrelated working-tree changes instead of staging them.
4. Verify that neither `v{version}` nor its GitHub Release already exists.
5. Confirm `.github/workflows/release.yml` still builds on pushed `v*` tags before relying on this workflow.

## Update and validate

1. Update the package version in `Cargo.toml` and the application package entry in `Cargo.lock`.
2. Search for other occurrences of the old application version and change only release metadata that must remain synchronized.
3. Run:

```powershell
cargo test
cargo fmt --check
cargo clippy -- -D warnings
cargo build --release
```

4. Stage only the version files and inspect `git diff --cached`.
5. Commit with the repository convention: `Bump version to v{version}`.

## Publish and merge

1. Push the current topic branch with upstream tracking.
2. Prefer the GitHub connector for PR writes and merging; use `gh` only for gaps.
3. Use a Chinese PR title and description covering the release contents and validation.
4. When an open feature PR still contains the version commit, update and merge that PR.
5. If the feature PR was merged before the version commit reached it, create a minimal version-only PR rather than tagging an older commit.
6. Before merging, verify the PR is ready, mergeable, clean, and still points at the expected head SHA.
7. Merge using repository conventions; never force-push unless the user explicitly authorizes it.

## Tag and release

1. Fetch `origin` again after the merge.
2. Read `Cargo.toml` from `origin/main` and verify it contains the confirmed version.
3. Resolve the exact merged `origin/main` SHA. Never tag the topic-branch head.
4. Recheck that the local tag, remote tag, and GitHub Release do not already exist.
5. Create annotated tag `v{version}` on that merge SHA and push only that tag.
6. Locate the tag-triggered Release workflow run and monitor it to a terminal state.
7. On failure, inspect the failing step and logs before changing anything; route CI repair through the GitHub CI-fix workflow when appropriate.
8. On success, verify the GitHub Release is published and the expected `codex-usage-taskbar-monitor.exe` asset is uploaded.

## Post-release branch cleanup

Only after the Release is published and the expected asset is verified:

1. Invoke the `git-clean-branches` skill to audit and conservatively clean stale branches.
2. Pass the release context to that skill: retained base `main`, released tag `v{version}`, the merged release PR, any merged feature PR branch just released, and `origin` as the writable personal remote.
3. Let `git-clean-branches` prove worktree usage, branch protection, authoritative reachability, exact-SHA mirrors, and age before deleting anything. Never delete authoritative/read-only refs, protected branches, active worktree branches, or branches whose safety cannot be proven.
4. If cleanup is blocked by ambiguous ownership, reachability, creation age, or unique history, keep the branch and report the exception instead of force-deleting it.
5. Include the cleanup audit, deleted local/remote branches, retained exceptions, and any cleanup validation result in the completion report.

## Completion report

Report the version commit, PR link, merge SHA, tag, workflow link and conclusion, Release link, asset name, SHA-256 digest, and post-release branch cleanup result. State which validation commands passed.

## Examples

- “升级应用版本” → inspect changes, recommend the next SemVer value, and wait for confirmation before editing.
- “发布 v1.6.1” → treat `1.6.1` as confirmed, then run the complete update, merge, tag, workflow, and asset-verification flow.
