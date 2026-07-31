[中文](../zh-CN/diagnostics-and-privacy.md) | [Back to English README](../../README_EN.md)

# Diagnostics and privacy

## Codex credentials

The application searches for the Codex CLI sign-in file in this order:

1. `%CODEX_HOME%\auth.json`, when `CODEX_HOME` is set
2. `%USERPROFILE%\.codex\auth.json`

It reads only the access token and account identifier required by the Codex usage request and does not directly edit `auth.json`. If the service reports an authentication error, the monitor attempts to run the local Codex CLI in the background to refresh the token, then reads the credentials again. If that still fails, usage polling pauses and a tray notification asks you to sign in again.

## Diagnostic log

Start the application with diagnostics enabled:

```powershell
codex-usage-taskbar-monitor --diagnose
```

The log is written to `%TEMP%\codex-usage-taskbar-monitor.log`, replacing the previous log on each diagnostic launch. It records runtime information for startup, polling, updates, and CodexRadar errors. Review local paths and error context before sharing it.

## Local files

| Path | Contents |
| --- | --- |
| `%APPDATA%\CodexUsageTaskbarMonitor\settings.json` | Widget position, refresh interval, language, display choices, and feature switches |
| `%APPDATA%\CodexUsageTaskbarMonitor\codexradar-cache.json` | Derived recommendations, cache timestamps, and request validators; complete CodexRadar responses are not retained |
| `%TEMP%\codex-usage-taskbar-monitor.log` | Diagnostic log created only when `--diagnose` is used |

## Network access

| Destination | When accessed | Data sent |
| --- | --- | --- |
| Codex usage endpoint | During normal usage polling | Codex access token, optional account identifier, and routine request metadata |
| GitHub | During automatic or manual version checks and portable updates | Routine network data needed to check releases or download an asset |
| CodexRadar | Only after it is explicitly enabled and recommendations refresh | Recommendation request and application User-Agent; no Codex Token, usage data, or project content |

The HTTP library honors configured proxy environment variables, so these requests may pass through your proxy. Remote services and proxies can still observe routine network metadata such as your IP address and User-Agent.

## Privacy boundaries

The application has no separate backend, does not upload project files, does not collect analytics, and does not send Codex credentials to CodexRadar. CodexRadar is a third-party community service; leave the feature disabled if you do not want the application to connect to it.
