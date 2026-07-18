# Repository Guidelines

## Project Structure & Module Organization

This Windows-only Rust application displays Codex usage in a taskbar widget and notification-area icon. `src/main.rs` owns startup and CLI dispatch:

- `src/window/`: widget lifetime, Win32 message handling, rendering, menus, layout, settings, and polling coordination
- `src/poller/`: Codex credential discovery, API calls, and display formatting
- `src/updater/`: GitHub/Winget release checks and installation logic
- `src/core/`: shared models and diagnostic logging
- `src/platform/`: Windows-native constants and theme helpers
- `src/localization/`: one module per supported language; update `mod.rs` when adding a locale
- `src/icons/`: application icon source files and generated Windows icon assets

Build metadata and dependencies live in `Cargo.toml`; `build.rs` embeds Windows resources.

## Build, Test, and Development Commands

Run commands from the repository root on Windows:

```powershell
cargo build                 # Compile the debug executable
cargo run -- --diagnose     # Run and write diagnostics to %TEMP%
cargo test                  # Run unit tests
cargo fmt --check           # Verify Rust formatting
cargo clippy -- -D warnings # Reject lint warnings
cargo build --release       # Produce the optimized distributable
```

Use `cargo fmt` before committing. Test widget, tray, or Win32 message changes on a real Windows taskbar.

## Coding Style & Naming Conventions

Use Rust 2021 idioms and `rustfmt` defaults (four-space indentation). Follow existing naming: `snake_case` for functions, modules, and files; `PascalCase` for types and enums; `SCREAMING_SNAKE_CASE` for constants. Prefer focused modules and minimal visibility (`pub(crate)`, `pub(super)`). Keep Win32 unsafe code narrow and document non-obvious invariants.

## Testing Guidelines

Place unit tests in a nearby `#[cfg(test)] mod tests` block. Name tests as observable behavior, e.g. `settings_without_usage_display_defaults_to_used`. Cover serialization defaults, formatting, and selection logic with unit tests. There is no configured coverage threshold; add regression tests for bug fixes when the behavior is testable without the Windows UI.

## Commit & Pull Request Guidelines

Match the existing history: use concise imperative subjects, preferably Conventional Commit scopes such as `fix(tray): fall back to weekly usage` or `refactor(window): split rendering`. Use `Bump version to vX.Y.Z` only for version releases. Keep commits focused. Pull requests should explain user-visible changes, testing performed, and linked issues; include screenshots or a short recording for taskbar, tray, menu, or layout changes.

## Security & Configuration

Never commit `%USERPROFILE%\\.codex\\auth.json`, tokens, logs, or local settings. The app reads local credentials and may use proxy environment variables, so redact secrets from diagnostics and issue reports.
