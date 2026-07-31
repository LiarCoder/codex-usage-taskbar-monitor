[简体中文](README.md) | [English](README_EN.md)

![Windows](https://img.shields.io/badge/platform-Windows-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# Codex 用量监控

![任务栏中的已使用用量显示](docs/images/percentage-bar-in-used-display.png)

一个轻量的 Windows Codex CLI 用量监控工具。它把当前用量和重置倒计时直接显示在任务栏上，方便随时查看。

## 主要功能

- 在任务栏中显示 Codex 5 小时和 7 天用量窗口；5 小时窗口已弃用，可能不会由 Codex 返回
- 支持“已使用”和“剩余”两种显示方式，以及节省空间的紧凑模式
- 可拖动定位、切换任务栏、通过托盘图标显隐，并自动适配系统主题和 DPI
- 支持刷新频率、开机启动、多语言、窗口选择和应用更新等设置
- 可选启用 [CodexRadar](https://codexradar.com/) 社区模型推荐；该功能默认关闭

## 运行要求

- Windows 10 或 Windows 11
- 已安装并登录 Codex CLI

## 快速开始

从 [Releases](https://github.com/LiarCoder/codex-usage-taskbar-monitor/releases) 下载 `codex-usage-taskbar-monitor.exe`，将它放在当前用户可写的目录中，然后运行：

```powershell
codex-usage-taskbar-monitor
```

左键单击托盘图标可以显示或隐藏小工具；右键单击小工具或托盘图标可以打开菜单。

## 文档

| 主题 | 内容 |
| --- | --- |
| [用量显示](docs/zh-CN/usage-display.md) | 用量窗口、百分比模式、重置倒计时、紧凑模式和托盘显示 |
| [小工具与设置](docs/zh-CN/widget-and-settings.md) | 任务栏定位、交互方式、刷新、语言、启动和更新设置 |
| [CodexRadar](docs/zh-CN/codex-radar.md) | 推荐来源、计算方式、缓存策略和启用方式 |
| [诊断与隐私](docs/zh-CN/diagnostics-and-privacy.md) | 凭据、日志、本地文件、联网行为和隐私边界 |

## 隐私

应用不会上传项目文件或收集分析数据，也不会直接修改 `auth.json`。CodexRadar 只有在明确启用后才会联网。详见[诊断与隐私](docs/zh-CN/diagnostics-and-privacy.md)。

## 许可证

[MIT](LICENSE)
