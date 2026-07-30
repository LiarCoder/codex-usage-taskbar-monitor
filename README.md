![Windows](https://img.shields.io/badge/platform-Windows-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# Codex Usage Monitor

![Screenshot](.github/animation.gif)

A lightweight Windows usage monitor for the Codex CLI. It displays available Codex usage windows with live reset countdowns, so your remaining quota is always visible.

## Requirements

- Windows 10 or Windows 11
- Codex CLI installed and signed in

The monitor reads credentials from `$CODEX_HOME/auth.json` or `~/.codex/auth.json`.

## Install and use

Download `codex-usage-taskbar-monitor.exe` from the [Releases](https://github.com/LiarCoder/codex-usage-taskbar-monitor/releases) page and place it in a user-writable folder. Run:

```powershell
codex-usage-taskbar-monitor
```

The taskbar Widget and its tray icon show Codex usage. Drag the Widget's left divider to adjust its position or move it to another taskbar. Triple-click anywhere outside the drag handle to open the CodexRadar website. Left-click the tray icon to show or hide the Widget.

The right-click menu retains update frequency, usage-display mode, startup, position reset, compact mode, language, update checks, and other application settings. Under **Settings > Usage windows**, choose whether the Widget shows the 7-day and 5-hour limits. The 5-hour limit is deprecated and may not be available from Codex. Compact Mode hides the percentage bars and shows only usage text to save taskbar space. Provider selection is intentionally omitted because Codex is always enabled. Choose **Used** or **Remaining** to control the percentages shown in the bars, badge, and tooltip.

## CodexRadar recommendations

CodexRadar support is an optional, experimental enhancement and is disabled by default. Enable it from **Settings > CodexRadar > Enable CodexRadar**. The first enable action shows a privacy notice before any CodexRadar request is sent. The same submenu can refresh recommendation data or open the CodexRadar website.

When enabled, pause over the Widget for about 500 ms to see a native tooltip. The left drag handle is excluded from the tooltip area. The tooltip shows three non-personalized community recommendations:

- **Radar** uses the `daily_development` value recommendation published by [CodexRadar](https://codexradar.com/)
- **IQ/$** considers combinations with raw IQ greater than or equal to 90 and positive average price, then maximizes `IQ / average_price_usd`
- **IQ-first** uses the same eligible combinations and maximizes `0.8 × (IQ / highest eligible IQ) + 0.2 × (lowest eligible price / price)`

Recommendation data is cached separately from Codex usage data. CodexRadar failures, stale data, or incompatible API changes do not affect the core usage monitor. These results are unofficial community data and are not filtered against the models available to your account, so a recommended model or effort combination might not be usable by your account.

## Diagnostics

```powershell
codex-usage-taskbar-monitor --diagnose
```

This writes `%TEMP%\codex-usage-taskbar-monitor.log`. Settings are stored at `%APPDATA%\CodexUsageTaskbarMonitor\settings.json`. Derived CodexRadar recommendations and request validators are stored separately in `%APPDATA%\CodexUsageTaskbarMonitor\codexradar-cache.json`; full CodexRadar responses are not retained.

## Privacy and security

The application reads your local Codex credentials and sends authenticated requests only to the Codex usage endpoint. It does not upload project files, collect analytics, use a separate backend, or directly edit `auth.json`.

If a token needs renewal, the monitor can invoke the local Codex CLI; the CLI performs any credential update. GitHub is contacted only by the existing release-update flow, and configured proxy environment variables may route outbound requests through your proxy.

CodexRadar is contacted only after you explicitly enable the feature. Those requests use public CodexRadar endpoints and do not include your Codex Token, usage data, project content, or other local data. CodexRadar and any configured proxy can still observe normal network metadata such as your IP address and the application's User-Agent.

## License

MIT.
