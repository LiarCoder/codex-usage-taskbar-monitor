![Windows](https://img.shields.io/badge/platform-Windows-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# Codex Usage Monitor

![Screenshot](.github/animation.gif)

A lightweight Windows usage monitor for the Codex CLI. It displays your current 5-hour and 7-day Codex rate-limit windows with live reset countdowns, so your remaining quota is always visible.

## Requirements

- Windows 10 or Windows 11
- Codex CLI installed and signed in

The monitor reads credentials from `$CODEX_HOME/auth.json` or `~/.codex/auth.json`.

## Install and use

Download `codex-usage-taskbar-monitor.exe` from the [Releases](https://github.com/LiarCoder/codex-usage-taskbar-monitor/releases) page and place it in a user-writable folder. Run:

```powershell
codex-usage-taskbar-monitor
```

The taskbar Widget and its tray icon show Codex usage. Drag the Widget's left divider to adjust its position or move it to another taskbar. Left-click the tray icon to show or hide the Widget.

The right-click menu retains update frequency, usage-display mode, startup, position reset, compact mode, language, update checks, and other application settings. Compact Mode hides the percentage bars and shows only usage text to save taskbar space. Provider selection is intentionally omitted because Codex is always enabled. Choose **Used** or **Remaining** to control the percentages shown in the bars, badge, and tooltip.

## Diagnostics

```powershell
codex-usage-taskbar-monitor --diagnose
```

This writes `%TEMP%\codex-usage-taskbar-monitor.log`. Settings are stored at `%APPDATA%\CodexUsageTaskbarMonitor\settings.json`.

## Privacy and security

The application reads your local Codex credentials and sends authenticated requests only to the Codex usage endpoint. It does not upload project files, collect analytics, use a separate backend, or directly edit `auth.json`.

If a token needs renewal, the monitor can invoke the local Codex CLI; the CLI performs any credential update. GitHub is contacted only by the existing release-update flow, and configured proxy environment variables may route outbound requests through your proxy.

## License

MIT.
