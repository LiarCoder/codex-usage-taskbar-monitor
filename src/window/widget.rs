//! Widget preferences and tray-icon state synchronization.

use super::*;

pub(super) fn save_state_settings() {
    let state = lock_state();
    if let Some(s) = state.as_ref() {
        save_settings(&SettingsFile {
            tray_offset: s.tray_offset,
            taskbar_index: s.taskbar_index,
            poll_interval_ms: s.poll_interval_ms,
            language: s
                .language_override
                .map(|language| language.code().to_string()),
            last_update_check_unix: s.last_update_check_unix,
            widget_visible: s.widget_visible,
            compact_mode: s.compact_mode,
            usage_display: s.usage_display,
            show_5hour_window: s.show_5hour_window,
            show_7day_window: s.show_7day_window,
        });
    }
}

fn tray_icon_data_from_state() -> Option<tray::TrayIconData> {
    let state = lock_state();
    match state.as_ref() {
        Some(s) if s.last_poll_ok => {
            let strings = s.language.strings();
            let mut tooltip_windows = Vec::new();
            if s.show_5hour_window && s.session_available {
                tooltip_windows.push(format!("5h: {}", s.session_text));
            }
            if s.show_7day_window && s.weekly_available {
                tooltip_windows.push(format!("7d: {}", s.weekly_text));
            }

            let (used_percent, display_percent) = match preferred_tray_window(
                s.show_5hour_window,
                s.session_available,
                s.show_7day_window,
                s.weekly_available,
            ) {
                Some(TrayUsageWindow::Session) => (
                    Some(s.session_percent),
                    Some(s.display_percentage(s.session_percent, true)),
                ),
                Some(TrayUsageWindow::Weekly) => (
                    Some(s.weekly_percent),
                    Some(s.display_percentage(s.weekly_percent, true)),
                ),
                None => (None, None),
            };
            let tooltip = if tooltip_windows.is_empty() {
                format!("{}: {}", strings.codex_model, strings.no_data)
            } else {
                format!("{} {}", strings.codex_model, tooltip_windows.join(" | "))
            };

            Some(tray::TrayIconData {
                used_percent,
                display_percent,
                tooltip,
            })
        }
        Some(s) => Some(tray::TrayIconData {
            used_percent: None,
            display_percent: None,
            tooltip: s.language.strings().window_title.to_string(),
        }),
        None => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrayUsageWindow {
    Session,
    Weekly,
}

pub(super) fn preferred_tray_window(
    show_5hour_window: bool,
    session_available: bool,
    show_7day_window: bool,
    weekly_available: bool,
) -> Option<TrayUsageWindow> {
    if show_5hour_window && session_available {
        Some(TrayUsageWindow::Session)
    } else if show_7day_window && weekly_available {
        Some(TrayUsageWindow::Weekly)
    } else {
        None
    }
}

pub(super) fn sync_tray_icons(hwnd: HWND) {
    if let Some(icon) = tray_icon_data_from_state() {
        tray::sync(hwnd, &icon);
    }
}

pub(super) fn toggle_widget_visibility(hwnd: HWND) {
    let new_visible = {
        let mut state = lock_state();
        if let Some(s) = state.as_mut() {
            s.widget_visible = !s.widget_visible;
            s.widget_visible
        } else {
            return;
        }
    };
    save_state_settings();
    unsafe {
        if new_visible {
            position_at_taskbar();
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            render_layered();
        } else {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}
