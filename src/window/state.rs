//! Shared in-process state for the taskbar widget.

use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;

use crate::core::models::{UsageData, UsageDisplayMode};
use crate::localization::LanguageId;
use crate::updater::{InstallChannel, ReleaseDescriptor};

/// Wrapper to make HWND sendable across threads (safe for PostMessage usage).
#[derive(Clone, Copy)]
pub(super) struct SendHwnd(isize);

unsafe impl Send for SendHwnd {}

impl SendHwnd {
    pub(super) fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }

    pub(super) fn to_hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

/// Shared application state.
pub(super) struct AppState {
    pub(super) hwnd: SendHwnd,
    pub(super) taskbar_hwnd: Option<HWND>,
    pub(super) tray_notify_hwnd: Option<HWND>,
    pub(super) win_event_hook: Option<HWINEVENTHOOK>,
    pub(super) is_dark: bool,
    pub(super) embedded: bool,
    pub(super) language_override: Option<LanguageId>,
    pub(super) language: LanguageId,
    pub(super) install_channel: InstallChannel,

    /// Codex 5h-window usage.
    pub(super) session_percent: f64,
    pub(super) session_text: String,
    pub(super) session_available: bool,
    /// Codex 7d-window usage.
    pub(super) weekly_percent: f64,
    pub(super) weekly_text: String,
    pub(super) weekly_available: bool,

    pub(super) usage_display: UsageDisplayMode,
    pub(super) data: Option<UsageData>,

    pub(super) poll_interval_ms: u32,
    pub(super) retry_count: u32,
    pub(super) force_notify_auth_error: bool,
    pub(super) auth_error_paused_polling: bool,
    pub(super) auth_watch_snapshot: String,
    pub(super) last_poll_ok: bool,
    pub(super) update_status: UpdateStatus,
    pub(super) last_update_check_unix: Option<u64>,

    pub(super) taskbar_index: usize,
    pub(super) tray_offset: i32,
    pub(super) dragging: bool,
    pub(super) drag_start_mouse_x: i32,
    pub(super) drag_start_client_x: i32,
    pub(super) drag_start_offset: i32,

    pub(super) widget_visible: bool,
    pub(super) compact_mode: bool,
    pub(super) show_5hour_window: bool,
    pub(super) show_7day_window: bool,
}

unsafe impl Send for AppState {}

impl AppState {
    pub(super) fn display_percentage(&self, used_percentage: f64, available: bool) -> f64 {
        display_percentage_for_availability(self.usage_display, used_percentage, available)
    }
}

pub(super) fn display_percentage_for_availability(
    usage_display: UsageDisplayMode,
    used_percentage: f64,
    available: bool,
) -> f64 {
    if available {
        usage_display.display_percentage(used_percentage)
    } else {
        used_percentage.clamp(0.0, 100.0)
    }
}

#[derive(Clone, Debug)]
pub(super) enum UpdateStatus {
    Idle,
    Checking,
    Applying,
    UpToDate,
    Available(ReleaseDescriptor),
}

static STATE: Mutex<Option<AppState>> = Mutex::new(None);

/// Lock state safely, recovering from a poisoned mutex.
pub(super) fn lock_state() -> MutexGuard<'static, Option<AppState>> {
    STATE.lock().unwrap_or_else(|error| error.into_inner())
}

pub(super) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
