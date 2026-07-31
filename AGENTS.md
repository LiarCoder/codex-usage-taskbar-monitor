# Repository Guidelines

## Project Layout

This is a Windows-only Rust 2021 application. `src/main.rs` handles startup, diagnostics, and updater CLI dispatch.

- `src/window/`: widget lifecycle, Win32 events, rendering, settings, polling, and UI coordination
- `src/tray/` and `src/platform/`: notification icon, native helpers, constants, and themes
- `src/poller/`: Codex credential discovery, usage API calls, and formatting
- `src/radar/`: CodexRadar fetching, validation, ranking, and caching
- `src/updater/`: GitHub release checks plus portable and WinGet updates
- `src/core/`: shared models and diagnostics
- `src/localization/` and `src/icons/`: translations and application assets

`build.rs` embeds Windows resources. Release automation lives in `.github/workflows/release.yml` and `.agents/skills/release-app-version/`.

## Common Commands

Run from the repository root on Windows:

```powershell
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build
cargo run -- --diagnose
```

Use `cargo build --release` only when validating a release. Manually test taskbar, tray, menu, DPI, and Explorer-restart behavior when those paths change.

## Development Conventions

- Follow `rustfmt`, Rust naming conventions, and the existing module boundaries
- Keep visibility minimal and Win32 `unsafe` blocks narrow; document non-obvious safety invariants
- Add unit tests near the implementation in `#[cfg(test)] mod tests`; add regression tests for testable fixes
- Update `src/localization/mod.rs` and relevant language modules when adding user-facing text
- Keep commits focused and use the repository's Conventional Commit style; reserve `Bump version to vX.Y.Z` for releases
- PR titles and descriptions should be in Chinese and include user-visible changes and validation performed

Never commit Codex credentials, tokens, logs, local settings, or generated release artifacts. Preserve CodexRadar's opt-in and privacy behavior. Follow `.agents/skills/release-app-version/SKILL.md` for version releases.
