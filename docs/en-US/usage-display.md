[中文](../zh-CN/usage-display.md) | [Back to English README](../../README_EN.md)

# Usage display

Codex Usage Monitor places the quota percentages and reset countdowns returned by Codex directly in the Windows taskbar. The Widget shows only windows that are currently available from the API and enabled in settings.

## Usage windows

- `7d`: the 7-day usage window
- `5h`: the deprecated 5-hour usage window. Codex might no longer return this window, so it can remain hidden even when enabled in settings

Choose the windows to show under **Settings > Usage windows**. At least one option remains selected so both windows cannot be disabled at the same time.

## Used and Remaining

**Settings > Usage Display** provides two modes:

- **Used**: the percentage and highlighted segments represent consumed usage; this is the default
- **Remaining**: the percentage and highlighted segments represent usage still available

Both modes include the time until reset and apply consistently to the taskbar Widget, tray-icon badge, and tray tooltip.

![Used mode](../images/percentage-bar-in-used-display.png)

![Remaining mode](../images/percentage-bar-in-remaining-display.png)

## Compact Mode

Compact Mode hides the segmented percentage bar and keeps only the window, percentage, and reset countdown. It is useful when taskbar space is limited.

![Compact Mode](../images/compact-mode.png)

## Tray display

The tray icon shows a percentage badge for the preferred available window: an enabled and available 5-hour window takes priority, otherwise the 7-day window is used. Hover over the tray icon to see usage for every enabled and available window.
