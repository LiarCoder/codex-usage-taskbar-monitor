use std::time::{Duration, SystemTime};

use crate::core::models::{UsageDisplayMode, UsageSection};
use crate::localization::Strings;

pub fn format_line(
    section: &UsageSection,
    strings: Strings,
    usage_display: UsageDisplayMode,
) -> String {
    let percentage = format!(
        "{:.0}%",
        usage_display.display_percentage(section.percentage)
    );
    let countdown = format_countdown(section.resets_at, strings);
    if countdown.is_empty() {
        percentage
    } else {
        format!("{percentage} · {countdown}")
    }
}

fn format_countdown(resets_at: Option<SystemTime>, strings: Strings) -> String {
    let Some(reset) = resets_at else {
        return String::new();
    };
    let remaining = match reset.duration_since(SystemTime::now()) {
        Ok(remaining) => remaining,
        Err(_) => return strings.now.to_string(),
    };
    format_countdown_from_secs(remaining.as_secs(), strings)
}

fn format_countdown_from_secs(total: u64, strings: Strings) -> String {
    if total >= 86_400 {
        format!("{}{}", total / 86_400, strings.day_suffix)
    } else if total >= 3_600 {
        format!("{}{}", total / 3_600, strings.hour_suffix)
    } else if total >= 60 {
        format!("{}{}", total / 60, strings.minute_suffix)
    } else {
        format!("{total}{}", strings.second_suffix)
    }
}

pub fn time_until_display_change(resets_at: Option<SystemTime>) -> Option<Duration> {
    let total = resets_at?.duration_since(SystemTime::now()).ok()?.as_secs();
    let bucket = if total >= 86_400 {
        total / 86_400 * 86_400
    } else if total >= 3_600 {
        total / 3_600 * 3_600
    } else if total >= 60 {
        total / 60 * 60
    } else {
        total
    };
    Some(Duration::from_secs(total.saturating_sub(bucket) + 1))
}
