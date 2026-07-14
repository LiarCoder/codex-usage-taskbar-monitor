//! Settings file persistence for widget preferences.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::models::UsageDisplayMode;

pub(crate) fn settings_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("CodexUsageTaskbarMonitor")
        .join("settings.json")
}

pub(crate) fn default_poll_interval() -> u32 {
    super::POLL_15_MIN
}

pub(crate) fn default_widget_visible() -> bool {
    true
}

pub(crate) fn default_show_usage_window() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SettingsFile {
    #[serde(default)]
    pub(crate) tray_offset: i32,
    #[serde(default)]
    pub(crate) taskbar_index: usize,
    #[serde(default = "default_poll_interval")]
    pub(crate) poll_interval_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_update_check_unix: Option<u64>,
    #[serde(default = "default_widget_visible")]
    pub(crate) widget_visible: bool,
    #[serde(default)]
    pub(crate) compact_mode: bool,
    #[serde(default)]
    pub(crate) usage_display: UsageDisplayMode,
    #[serde(default = "default_show_usage_window")]
    pub(crate) show_5hour_window: bool,
    #[serde(default = "default_show_usage_window")]
    pub(crate) show_7day_window: bool,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            tray_offset: 0,
            taskbar_index: 0,
            poll_interval_ms: default_poll_interval(),
            language: None,
            last_update_check_unix: None,
            widget_visible: true,
            compact_mode: false,
            usage_display: UsageDisplayMode::Used,
            show_5hour_window: true,
            show_7day_window: true,
        }
    }
}

pub(crate) fn load_settings() -> SettingsFile {
    let content = match std::fs::read_to_string(settings_path()) {
        Ok(c) => c,
        Err(_) => return SettingsFile::default(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

pub(crate) fn save_settings(settings: &SettingsFile) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}
