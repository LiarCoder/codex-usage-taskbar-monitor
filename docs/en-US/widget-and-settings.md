[中文](../zh-CN/widget-and-settings.md) | [Back to English README](../../README_EN.md)

# Widget and settings

## Taskbar position and interactions

- Drag the vertical handle on the left side of the Widget to reposition it in the taskbar
- Drop the Widget on another monitor's taskbar to move it between taskbars
- Left-click the tray icon to show or hide the Widget
- Right-click the Widget or tray icon to open the menu
- When CodexRadar is enabled, triple-click anywhere outside the left handle to open the CodexRadar website

The position, target taskbar, and visibility state are saved in settings. Use **Settings > Reset Position** to return the Widget to its default position.

## Menu settings

![Settings menu](../images/settings.png)

| Setting | Behavior |
| --- | --- |
| Refresh | Reads Codex usage immediately |
| Update Frequency | Chooses 1 minute, 5 minutes, 15 minutes, or 1 hour; the default is 15 minutes |
| Usage Display | Switches between Used and Remaining |
| Usage windows | Controls the 5-hour and 7-day windows |
| Compact Mode | Hides the segmented percentage bar and reduces Widget width |
| Start with Windows | Adds or removes the current-user Windows startup entry |
| Language | Follows the system language or uses a selected built-in language |
| GitHub | Opens the project's GitHub repository |
| Show Widget | Toggles the taskbar Widget without exiting the application |

The application follows the Windows light or dark theme and DPI. It repositions and rerenders after display, DPI, system-theme, or Explorer state changes.

## Application updates

The application checks GitHub Releases automatically once per day. You can also check manually from the version menu item. When an update is available:

- The portable build downloads the new executable, uses an updater helper to replace the running version, and restarts
- A WinGet installation invokes WinGet to upgrade the package and then restarts

Settings are stored in `%APPDATA%\CodexUsageTaskbarMonitor\settings.json`.
