use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageDisplayMode {
    #[default]
    Used,
    Remaining,
}

impl UsageDisplayMode {
    pub fn display_percentage(self, used_percentage: f64) -> f64 {
        let used_percentage = used_percentage.clamp(0.0, 100.0);
        match self {
            Self::Used => used_percentage,
            Self::Remaining => 100.0 - used_percentage,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct UsageSection {
    pub percentage: f64,
    pub resets_at: Option<SystemTime>,
}

#[derive(Clone, Debug, Default)]
pub struct UsageData {
    pub session: UsageSection,
    pub weekly: UsageSection,
}

/// Codex is the only supported provider, so the app-level usage payload is
/// just a `UsageData` alias. This used to be a struct with one `Option<UsageData>`
/// field per provider (primary / codex / secondary) before the multi-provider
/// UI was removed.
pub type AppUsageData = UsageData;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_display_mode_converts_and_clamps_percentages() {
        for (used, expected_used, expected_remaining) in [
            (-5.0, 0.0, 100.0),
            (0.0, 0.0, 100.0),
            (42.0, 42.0, 58.0),
            (100.0, 100.0, 0.0),
            (105.0, 100.0, 0.0),
        ] {
            assert_eq!(
                UsageDisplayMode::Used.display_percentage(used),
                expected_used
            );
            assert_eq!(
                UsageDisplayMode::Remaining.display_percentage(used),
                expected_remaining
            );
        }
    }
}
