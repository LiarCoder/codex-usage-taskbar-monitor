[返回中文 README](../../README.md) | [English](../en-US/diagnostics-and-privacy.md)

# 诊断与隐私

## Codex 凭据

应用按以下顺序查找 Codex CLI 的登录文件：

1. `%CODEX_HOME%\auth.json`，前提是设置了 `CODEX_HOME`
2. `%USERPROFILE%\.codex\auth.json`

应用只读取其中的访问令牌和账户标识，用于请求 Codex 用量接口，不会直接修改 `auth.json`。如果服务返回身份验证错误，应用会尝试在后台调用本地 Codex CLI 刷新令牌，然后重新读取凭据；仍然失败时会暂停用量轮询并通过托盘通知提示重新登录。

## 诊断日志

使用诊断参数启动应用：

```powershell
codex-usage-taskbar-monitor --diagnose
```

日志写入 `%TEMP%\codex-usage-taskbar-monitor.log`，每次诊断启动都会覆盖旧日志。日志用于记录启动、轮询、更新和 CodexRadar 错误等运行信息；分享前请自行检查其中的本地路径和错误上下文。

## 本地文件

| 路径 | 内容 |
| --- | --- |
| `%APPDATA%\CodexUsageTaskbarMonitor\settings.json` | 小工具位置、刷新频率、语言、显示方式和功能开关 |
| `%APPDATA%\CodexUsageTaskbarMonitor\codexradar-cache.json` | 派生后的推荐结果、缓存时间和请求校验值；不保存完整 CodexRadar 响应 |
| `%TEMP%\codex-usage-taskbar-monitor.log` | 仅在使用 `--diagnose` 时生成的诊断日志 |

## 联网行为

| 目标 | 何时访问 | 发送内容 |
| --- | --- | --- |
| Codex 用量接口 | 正常用量轮询 | Codex 访问令牌、可选账户标识和应用请求信息 |
| GitHub | 自动或手动检查版本、便携版下载更新时 | 版本检查或发布文件请求所需的常规网络信息 |
| CodexRadar | 仅在明确启用后刷新推荐时 | 推荐数据请求和应用 User-Agent；不包含 Codex Token、用量或项目内容 |

依赖库会读取系统中配置的代理环境变量，因此这些请求可能经过你的代理。任何远端服务和代理仍可看到 IP 地址、User-Agent 等常规网络元数据。

## 隐私边界

应用没有独立后端，不上传项目文件，不收集分析数据，也不会把 Codex 凭据发送给 CodexRadar。CodexRadar 是第三方社区服务；如不希望与其建立连接，请保持该功能关闭。
