//! Context menu item IDs and version-action label.

use crate::localization::{self, LanguageId, Strings};
use crate::updater::InstallChannel;

use super::UpdateStatus;

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
