use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::core::diagnose;
use crate::core::models::UsageDisplayMode;
use crate::localization::{self, LanguageId, Strings};
use crate::platform::native::{
    self, TIMER_COUNTDOWN, TIMER_POLL, TIMER_RESET_POLL, TIMER_UPDATE_CHECK, WM_APP_TRAY,
    WM_APP_USAGE_UPDATED,
};
use crate::platform::theme;
use crate::poller;
use crate::tray;
use crate::updater::{self, InstallChannel, ReleaseDescriptor, UpdateCheckResult};

mod layout;
use layout::*;
mod settings;
use settings::*;
mod render;
use render::*;
mod positioning;
use positioning::*;
mod menu;
use menu::*;
mod state;
use state::*;
mod startup;
use startup::*;
mod polling;
use polling::*;
mod updates;
use updates::*;
mod runtime;
mod taskbar;
use taskbar::*;
mod events;
mod widget;
use widget::*;

const RETRY_BASE_MS: u32 = 30_000; // 30 seconds

const POLL_1_MIN: u32 = 60_000;
const POLL_5_MIN: u32 = 300_000;
pub(crate) const POLL_15_MIN: u32 = 900_000;
const POLL_1_HOUR: u32 = 3_600_000;

const WM_DPICHANGED_MSG: u32 = 0x02E0;
const WM_APP_UPDATE_CHECK_COMPLETE: u32 = WM_APP + 2;
const TRAY_ICON_UPDATE_REPOSITION_SUPPRESS_MS: u64 = 750;

/// How often the watchdog thread polls for an explorer.exe restart (which
/// recreates the taskbar and wipes our tray-icon registration).
const TASKBAR_WATCH_INTERVAL_SECS: u64 = 2;

static SUPPRESS_TRAY_REPOSITION_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

/// Spacing below which two relaunches are treated as a storm (e.g. explorer.exe
/// crash-looping); when detected we back off instead of spawning in a tight loop.
const RELAUNCH_THROTTLE_SECS: u64 = 10;
const RELAUNCH_BACKOFF_SECS: u64 = 30;
/// Environment flag set on a relaunched child so it waits for the previous
/// instance's single-instance mutex instead of exiting immediately.
const ENV_RELAUNCH: &str = "CCUM_RELAUNCH";
/// Unix timestamp (seconds) of the relaunch that spawned this process, passed to
/// the child so it can detect a relaunch storm.
const ENV_LAST_RELAUNCH_UNIX: &str = "CCUM_LAST_RELAUNCH_UNIX";

pub fn run() {
    runtime::run();
}

/// Main window procedure. The message order and result values live in events.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    events::dispatch(hwnd, msg, wparam, lparam)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_without_usage_display_defaults_to_used() {
        let settings: SettingsFile = serde_json::from_str(r#"{"tray_offset": 12}"#).unwrap();

        assert_eq!(settings.tray_offset, 12);
        assert_eq!(settings.usage_display, UsageDisplayMode::Used);
        assert!(settings.show_5hour_window);
        assert!(settings.show_7day_window);
    }

    #[test]
    fn unavailable_usage_is_not_inverted_in_remaining_mode() {
        assert_eq!(
            display_percentage_for_availability(UsageDisplayMode::Remaining, 0.0, false),
            0.0
        );
        assert_eq!(
            display_percentage_for_availability(UsageDisplayMode::Remaining, 42.0, true),
            58.0
        );
    }

    #[test]
    fn remaining_usage_display_round_trips_through_settings_json() {
        let settings = SettingsFile {
            usage_display: UsageDisplayMode::Remaining,
            show_5hour_window: false,
            ..SettingsFile::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""usage_display":"remaining""#));

        let decoded: SettingsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.usage_display, UsageDisplayMode::Remaining);
        assert!(!decoded.show_5hour_window);
        assert!(decoded.show_7day_window);
    }

    #[test]
    fn usage_window_selection_is_persisted() {
        let settings = SettingsFile::default();
        let json = serde_json::to_string(&settings).unwrap();

        assert!(json.contains(r#""show_5hour_window":true"#));
        assert!(json.contains(r#""show_7day_window":true"#));
    }

    #[test]
    fn tray_icon_prefers_the_5_hour_window_when_it_is_visible_and_available() {
        assert_eq!(
            preferred_tray_window(true, true, true, true),
            Some(TrayUsageWindow::Session)
        );
    }

    #[test]
    fn tray_icon_falls_back_to_the_7_day_window_when_5_hour_is_hidden_or_missing() {
        assert_eq!(
            preferred_tray_window(false, true, true, true),
            Some(TrayUsageWindow::Weekly)
        );
        assert_eq!(
            preferred_tray_window(true, false, true, true),
            Some(TrayUsageWindow::Weekly)
        );
    }
}
