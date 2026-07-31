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
    format_countdown_from_secs(remaining.as_secs())
}

fn format_countdown_from_secs(total: u64) -> String {
    if total >= 86_400 {
        format_two_units(total / 86_400, 'd', total % 86_400 / 3_600, 'h')
    } else if total >= 3_600 {
        format_two_units(total / 3_600, 'h', total % 3_600 / 60, 'm')
    } else if total >= 60 {
        format_two_units(total / 60, 'm', total % 60, 's')
    } else {
        format!("{total}s")
    }
}

fn format_two_units(
    primary: u64,
    primary_suffix: char,
    secondary: u64,
    secondary_suffix: char,
) -> String {
    if secondary == 0 {
        format!("{primary}{primary_suffix}")
    } else {
        format!("{primary}{primary_suffix}{secondary}{secondary_suffix}")
    }
}

pub fn time_until_display_change(resets_at: Option<SystemTime>) -> Option<Duration> {
    let total = resets_at?.duration_since(SystemTime::now()).ok()?.as_secs();
    Some(display_change_delay_from_secs(total))
}

fn display_change_delay_from_secs(total: u64) -> Duration {
    let bucket = if total >= 86_400 {
        total / 3_600 * 3_600
    } else if total >= 3_600 {
        total / 60 * 60
    } else {
        total
    };
    Duration::from_secs(total.saturating_sub(bucket) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_countdowns_with_two_compact_ascii_units() {
        assert_eq!(format_countdown_from_secs(4 * 86_400 + 17 * 3_600), "4d17h");
        assert_eq!(format_countdown_from_secs(4 * 86_400), "4d");
        assert_eq!(format_countdown_from_secs(17 * 3_600 + 25 * 60), "17h25m");
        assert_eq!(format_countdown_from_secs(17 * 3_600), "17h");
        assert_eq!(format_countdown_from_secs(25 * 60 + 49), "25m49s");
        assert_eq!(format_countdown_from_secs(25 * 60), "25m");
        assert_eq!(format_countdown_from_secs(49), "49s");
    }

    #[test]
    fn schedules_updates_for_the_smallest_displayed_unit() {
        assert_eq!(
            display_change_delay_from_secs(4 * 86_400 + 17 * 3_600 + 25 * 60 + 49),
            Duration::from_secs(25 * 60 + 50)
        );
        assert_eq!(
            display_change_delay_from_secs(17 * 3_600 + 25 * 60 + 49),
            Duration::from_secs(50)
        );
        assert_eq!(
            display_change_delay_from_secs(25 * 60 + 49),
            Duration::from_secs(1)
        );
    }
}
