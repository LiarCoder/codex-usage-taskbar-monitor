//! Context menu item IDs and version-action label.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::core::models::UsageDisplayMode;
use crate::localization::{self, LanguageId, Strings};
use crate::platform::native;
use crate::tray;
use crate::updater::InstallChannel;

use super::{
    is_startup_enabled, lock_state, UpdateStatus, POLL_15_MIN, POLL_1_HOUR, POLL_1_MIN, POLL_5_MIN,
};

// Menu item IDs for update frequency
pub(crate) const IDM_FREQ_1MIN: u16 = 10;
pub(crate) const IDM_FREQ_5MIN: u16 = 11;
pub(crate) const IDM_FREQ_15MIN: u16 = 12;
pub(crate) const IDM_FREQ_1HOUR: u16 = 13;
pub(crate) const IDM_START_WITH_WINDOWS: u16 = 20;
pub(crate) const IDM_RESET_POSITION: u16 = 30;
pub(crate) const IDM_VERSION_ACTION: u16 = 31;
pub(crate) const IDM_COMPACT_MODE: u16 = 32;
pub(crate) const IDM_SHOW_5HOUR_WINDOW: u16 = 33;
pub(crate) const IDM_SHOW_7DAY_WINDOW: u16 = 34;
pub(crate) const IDM_LANG_SYSTEM: u16 = 40;
pub(crate) const IDM_LANG_ENGLISH: u16 = 41;
pub(crate) const IDM_LANG_DUTCH: u16 = 42;
pub(crate) const IDM_LANG_SPANISH: u16 = 43;
pub(crate) const IDM_LANG_FRENCH: u16 = 44;
pub(crate) const IDM_LANG_GERMAN: u16 = 45;
pub(crate) const IDM_LANG_JAPANESE: u16 = 46;
pub(crate) const IDM_LANG_KOREAN: u16 = 47;
pub(crate) const IDM_LANG_TRADITIONAL_CHINESE: u16 = 48;
pub(crate) const IDM_LANG_SIMPLIFIED_CHINESE: u16 = 51;
pub(crate) const IDM_LANG_RUSSIAN: u16 = 49;
pub(crate) const IDM_LANG_PORTUGUESE_BRAZIL: u16 = 50;
pub(crate) const IDM_USAGE_DISPLAY_USED: u16 = 80;
pub(crate) const IDM_USAGE_DISPLAY_REMAINING: u16 = 81;

/// Builds the human-readable version / update status string shown in the context menu.
pub(crate) fn version_action_label(
    strings: Strings,
    language: LanguageId,
    install_channel: InstallChannel,
    status: &UpdateStatus,
) -> String {
    let current = env!("CARGO_PKG_VERSION");
    match status {
        UpdateStatus::Idle => format!("v{current} - {}", strings.check_for_updates),
        UpdateStatus::Checking => format!("v{current} - {}", strings.checking_for_updates),
        UpdateStatus::Applying => format!("v{current} - {}", strings.applying_update),
        UpdateStatus::UpToDate => format!("v{current} - {}", strings.up_to_date_short),
        UpdateStatus::Available(release) => match install_channel {
            InstallChannel::Portable => {
                format!(
                    "v{current} - {} v{}",
                    strings.update_to, release.latest_version
                )
            }
            InstallChannel::Winget => format!(
                "v{current} - {} v{}",
                localization::update_via_winget(language),
                release.latest_version
            ),
        },
    }
}

pub(super) fn show_context_menu(hwnd: HWND) {
    unsafe {
        let (
            current_interval,
            strings,
            language,
            language_override,
            install_channel,
            update_status,
            widget_visible,
            compact_mode,
            usage_display,
            show_5hour_window,
            show_7day_window,
        ) = {
            let state = lock_state();
            match state.as_ref() {
                Some(s) => (
                    s.poll_interval_ms,
                    s.language.strings(),
                    s.language,
                    s.language_override,
                    s.install_channel,
                    s.update_status.clone(),
                    s.widget_visible,
                    s.compact_mode,
                    s.usage_display,
                    s.show_5hour_window,
                    s.show_7day_window,
                ),
                None => (
                    POLL_15_MIN,
                    LanguageId::English.strings(),
                    LanguageId::English,
                    None,
                    InstallChannel::Portable,
                    UpdateStatus::Idle,
                    true,
                    false,
                    UsageDisplayMode::Used,
                    true,
                    true,
                ),
            }
        };

        let menu = CreatePopupMenu().unwrap();

        let refresh_str = native::wide_str(strings.refresh);
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            1,
            PCWSTR::from_raw(refresh_str.as_ptr()),
        );

        // Update Frequency submenu
        let freq_menu = CreatePopupMenu().unwrap();
        let freq_items: [(u16, u32, &str); 4] = [
            (IDM_FREQ_1MIN, POLL_1_MIN, strings.one_minute),
            (IDM_FREQ_5MIN, POLL_5_MIN, strings.five_minutes),
            (IDM_FREQ_15MIN, POLL_15_MIN, strings.fifteen_minutes),
            (IDM_FREQ_1HOUR, POLL_1_HOUR, strings.one_hour),
        ];
        for (id, interval, label) in freq_items {
            let label_str = native::wide_str(label);
            let flags = if interval == current_interval {
                MF_CHECKED
            } else {
                MENU_ITEM_FLAGS(0)
            };
            let _ = AppendMenuW(
                freq_menu,
                flags,
                id as usize,
                PCWSTR::from_raw(label_str.as_ptr()),
            );
        }

        let freq_label = native::wide_str(strings.update_frequency);

        // Usage display submenu
        let usage_display_menu = CreatePopupMenu().unwrap();
        let used_usage_label = native::wide_str(strings.used_usage);
        let used_usage_flags = if usage_display == UsageDisplayMode::Used {
            MF_CHECKED
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            usage_display_menu,
            used_usage_flags,
            IDM_USAGE_DISPLAY_USED as usize,
            PCWSTR::from_raw(used_usage_label.as_ptr()),
        );

        let remaining_usage_label = native::wide_str(strings.remaining_usage);
        let remaining_usage_flags = if usage_display == UsageDisplayMode::Remaining {
            MF_CHECKED
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            usage_display_menu,
            remaining_usage_flags,
            IDM_USAGE_DISPLAY_REMAINING as usize,
            PCWSTR::from_raw(remaining_usage_label.as_ptr()),
        );

        let usage_display_label = native::wide_str(strings.usage_display);

        let usage_windows_menu = CreatePopupMenu().unwrap();
        let five_hour_label = native::wide_str(strings.show_5hour_window);
        let five_hour_flags = if show_5hour_window {
            if !show_7day_window {
                MF_CHECKED | MF_GRAYED
            } else {
                MF_CHECKED
            }
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            usage_windows_menu,
            five_hour_flags,
            IDM_SHOW_5HOUR_WINDOW as usize,
            PCWSTR::from_raw(five_hour_label.as_ptr()),
        );

        let seven_day_label = native::wide_str(strings.show_7day_window);
        let seven_day_flags = if show_7day_window {
            if !show_5hour_window {
                MF_CHECKED | MF_GRAYED
            } else {
                MF_CHECKED
            }
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            usage_windows_menu,
            seven_day_flags,
            IDM_SHOW_7DAY_WINDOW as usize,
            PCWSTR::from_raw(seven_day_label.as_ptr()),
        );

        let usage_windows_label = native::wide_str(strings.usage_windows);

        // Settings submenu
        let settings_menu = CreatePopupMenu().unwrap();

        let startup_str = native::wide_str(strings.start_with_windows);
        let startup_flags = if is_startup_enabled() {
            MF_CHECKED
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            settings_menu,
            startup_flags,
            IDM_START_WITH_WINDOWS as usize,
            PCWSTR::from_raw(startup_str.as_ptr()),
        );

        let reset_pos_str = native::wide_str(strings.reset_position);
        let _ = AppendMenuW(
            settings_menu,
            MENU_ITEM_FLAGS(0),
            IDM_RESET_POSITION as usize,
            PCWSTR::from_raw(reset_pos_str.as_ptr()),
        );

        let compact_mode_str = native::wide_str(strings.compact_mode);
        let compact_mode_flags = if compact_mode {
            MF_CHECKED
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            settings_menu,
            compact_mode_flags,
            IDM_COMPACT_MODE as usize,
            PCWSTR::from_raw(compact_mode_str.as_ptr()),
        );

        let language_menu = CreatePopupMenu().unwrap();
        let system_label = native::wide_str(strings.system_default);
        let system_flags = if language_override.is_none() {
            MF_CHECKED
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            language_menu,
            system_flags,
            IDM_LANG_SYSTEM as usize,
            PCWSTR::from_raw(system_label.as_ptr()),
        );

        for language in LanguageId::ALL {
            let id = match language {
                LanguageId::English => IDM_LANG_ENGLISH,
                LanguageId::Dutch => IDM_LANG_DUTCH,
                LanguageId::Spanish => IDM_LANG_SPANISH,
                LanguageId::French => IDM_LANG_FRENCH,
                LanguageId::German => IDM_LANG_GERMAN,
                LanguageId::Japanese => IDM_LANG_JAPANESE,
                LanguageId::Korean => IDM_LANG_KOREAN,
                LanguageId::TraditionalChinese => IDM_LANG_TRADITIONAL_CHINESE,
                LanguageId::SimplifiedChinese => IDM_LANG_SIMPLIFIED_CHINESE,
                LanguageId::Russian => IDM_LANG_RUSSIAN,
                LanguageId::PortugueseBrazil => IDM_LANG_PORTUGUESE_BRAZIL,
            };
            let label_str = native::wide_str(language.native_name());
            let flags = if language_override == Some(language) {
                MF_CHECKED
            } else {
                MENU_ITEM_FLAGS(0)
            };
            let _ = AppendMenuW(
                language_menu,
                flags,
                id as usize,
                PCWSTR::from_raw(label_str.as_ptr()),
            );
        }

        let language_label = native::wide_str(strings.language);
        let _ = AppendMenuW(
            settings_menu,
            MF_POPUP,
            language_menu.0 as usize,
            PCWSTR::from_raw(language_label.as_ptr()),
        );

        let _ = AppendMenuW(
            settings_menu,
            MF_POPUP,
            freq_menu.0 as usize,
            PCWSTR::from_raw(freq_label.as_ptr()),
        );

        let _ = AppendMenuW(
            settings_menu,
            MF_POPUP,
            usage_display_menu.0 as usize,
            PCWSTR::from_raw(usage_display_label.as_ptr()),
        );

        let _ = AppendMenuW(
            settings_menu,
            MF_POPUP,
            usage_windows_menu.0 as usize,
            PCWSTR::from_raw(usage_windows_label.as_ptr()),
        );

        let _ = AppendMenuW(settings_menu, MF_SEPARATOR, 0, PCWSTR::null());

        let version_label =
            version_action_label(strings, language, install_channel, &update_status);
        let version_str = native::wide_str(&version_label);
        let version_flags = if matches!(
            update_status,
            UpdateStatus::Checking | UpdateStatus::Applying
        ) {
            MF_GRAYED
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            settings_menu,
            version_flags,
            IDM_VERSION_ACTION as usize,
            PCWSTR::from_raw(version_str.as_ptr()),
        );

        let settings_label = native::wide_str(strings.settings);
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            settings_menu.0 as usize,
            PCWSTR::from_raw(settings_label.as_ptr()),
        );

        let widget_label = native::wide_str(strings.show_widget);
        let widget_flags = if widget_visible {
            MF_CHECKED
        } else {
            MENU_ITEM_FLAGS(0)
        };
        let _ = AppendMenuW(
            menu,
            widget_flags,
            tray::IDM_TOGGLE_WIDGET as usize,
            PCWSTR::from_raw(widget_label.as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        let exit_str = native::wide_str(strings.exit);
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            2,
            PCWSTR::from_raw(exit_str.as_ptr()),
        );

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}
