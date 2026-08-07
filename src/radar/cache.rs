use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    ComputedRecommendations, FetchValidators, RadarRecommendations, RadarRefreshResult,
    SourceUpdate,
};

const CACHE_SCHEMA: u32 = 1;
pub(crate) const CACHE_MAX_AGE_SECS: u64 = 24 * 60 * 60;
pub(crate) const FULL_REFRESH_INTERVAL_SECS: u64 = 60 * 60;
pub(crate) const MANUAL_REFRESH_COOLDOWN_SECS: u64 = 60;
const MAX_RETRY_DELAY_SECS: u64 = 6 * 60 * 60;
const RETRY_DELAYS_SECS: [u64; 4] = [15 * 60, 30 * 60, 60 * 60, MAX_RETRY_DELAY_SECS];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct CachedSource<T> {
    pub(crate) value: T,
    pub(crate) validated_at_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) validator: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct RadarCache {
    schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) radar: Option<CachedSource<RadarRecommendations>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) computed: Option<CachedSource<ComputedRecommendations>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_attempt_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_complete_success_unix: Option<u64>,
    #[serde(default)]
    pub(crate) retry_count: u32,
    #[serde(default)]
    pub(crate) last_refresh_complete: bool,
}

impl Default for RadarCache {
    fn default() -> Self {
        Self {
            schema: CACHE_SCHEMA,
            radar: None,
            computed: None,
            last_attempt_unix: None,
            last_complete_success_unix: None,
            retry_count: 0,
            last_refresh_complete: false,
        }
    }
}

impl RadarCache {
    pub(crate) fn has_any_data(&self) -> bool {
        self.radar.is_some() || self.computed.is_some()
    }

    pub(crate) fn validators(&self) -> FetchValidators {
        FetchValidators {
            radar_last_modified: self
                .radar
                .as_ref()
                .and_then(|source| source.validator.clone()),
            efficiency_etag: self
                .computed
                .as_ref()
                .and_then(|source| source.validator.clone()),
        }
    }

    pub(crate) fn prune_stale(&mut self, now_unix: u64) {
        if self.radar.as_ref().is_some_and(|source| {
            now_unix.saturating_sub(source.validated_at_unix) > CACHE_MAX_AGE_SECS
        }) {
            self.radar = None;
        }
        if self.computed.as_ref().is_some_and(|source| {
            now_unix.saturating_sub(source.validated_at_unix) > CACHE_MAX_AGE_SECS
        }) {
            self.computed = None;
        }
    }
}

pub(crate) fn load_cache(now_unix: u64) -> RadarCache {
    let bytes = match std::fs::read(cache_path()) {
        Ok(bytes) => bytes,
        Err(_) => return RadarCache::default(),
    };
    parse_cache(&bytes, now_unix).unwrap_or_default()
}

pub(crate) fn save_cache(cache: &RadarCache) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(cache) {
        let _ = std::fs::write(path, json);
    }
}

pub(crate) fn apply_refresh(
    cache: &mut RadarCache,
    refresh: RadarRefreshResult,
    now_unix: u64,
) -> bool {
    let radar_succeeded = apply_source(&mut cache.radar, refresh.radar, now_unix);
    let computed_succeeded = apply_source(&mut cache.computed, refresh.computed, now_unix);
    let complete = radar_succeeded && computed_succeeded;

    cache.last_attempt_unix = Some(now_unix);
    cache.last_refresh_complete = complete;
    if complete {
        cache.last_complete_success_unix = Some(now_unix);
        cache.retry_count = 0;
    } else {
        cache.retry_count = cache.retry_count.saturating_add(1);
    }
    cache.prune_stale(now_unix);
    complete
}

pub(crate) fn next_due_in_secs(cache: &RadarCache, now_unix: u64) -> u64 {
    let delay = if cache.last_refresh_complete {
        FULL_REFRESH_INTERVAL_SECS
    } else if cache.retry_count > 0 {
        retry_delay(cache.retry_count)
    } else if cache.last_attempt_unix.is_some() {
        MANUAL_REFRESH_COOLDOWN_SECS
    } else {
        return 0;
    };
    let due_at = cache.last_attempt_unix.unwrap_or(0).saturating_add(delay);
    due_at.saturating_sub(now_unix)
}

fn parse_cache(bytes: &[u8], now_unix: u64) -> Option<RadarCache> {
    let mut cache: RadarCache = serde_json::from_slice(bytes).ok()?;
    if cache.schema != CACHE_SCHEMA {
        return None;
    }
    cache.prune_stale(now_unix);
    Some(cache)
}

fn apply_source<T>(
    cached: &mut Option<CachedSource<T>>,
    update: SourceUpdate<T>,
    now_unix: u64,
) -> bool {
    match update {
        SourceUpdate::Updated { value, validator } => {
            *cached = Some(CachedSource {
                value,
                validated_at_unix: now_unix,
                validator,
            });
            true
        }
        SourceUpdate::NotModified => {
            if let Some(source) = cached.as_mut() {
                source.validated_at_unix = now_unix;
                true
            } else {
                false
            }
        }
        SourceUpdate::Failed(_) => false,
    }
}

fn retry_delay(retry_count: u32) -> u64 {
    let index = retry_count.saturating_sub(1) as usize;
    RETRY_DELAYS_SECS[index.min(RETRY_DELAYS_SECS.len() - 1)]
}

fn cache_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("CodexUsageTaskbarMonitor")
        .join("codexradar-cache.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radar::Recommendation;

    fn recommendation(model: &str) -> Recommendation {
        Recommendation {
            model: model.to_string(),
            effort: "high".to_string(),
            iq: 95.0,
            average_cost_usd: 5.0,
            valid_tasks: 10,
        }
    }

    fn computed() -> ComputedRecommendations {
        ComputedRecommendations {
            iq_per_dollar: Some(recommendation("value")),
            intelligence_weighted: Some(recommendation("weighted")),
        }
    }

    fn radar_recommendations(model: &str) -> RadarRecommendations {
        RadarRecommendations {
            speed: Some(recommendation(model)),
            smart: None,
            daily_development: Vec::new(),
        }
    }

    #[test]
    fn refreshes_sources_independently_and_marks_partial_failures() {
        let mut cache = RadarCache::default();
        let complete = apply_refresh(
            &mut cache,
            RadarRefreshResult {
                radar: SourceUpdate::Updated {
                    value: radar_recommendations("radar"),
                    validator: Some("date".to_string()),
                },
                computed: SourceUpdate::Failed("offline".to_string()),
            },
            100,
        );

        assert!(!complete);
        assert_eq!(cache.radar.unwrap().value.speed.unwrap().model, "radar");
        assert!(cache.computed.is_none());
        assert_eq!(cache.retry_count, 1);
        assert!(!cache.last_refresh_complete);
    }

    #[test]
    fn not_modified_refreshes_existing_cache_age() {
        let mut cache = RadarCache {
            radar: Some(CachedSource {
                value: radar_recommendations("radar"),
                validated_at_unix: 10,
                validator: Some("date".to_string()),
            }),
            computed: Some(CachedSource {
                value: computed(),
                validated_at_unix: 10,
                validator: Some("etag".to_string()),
            }),
            ..RadarCache::default()
        };

        let complete = apply_refresh(
            &mut cache,
            RadarRefreshResult {
                radar: SourceUpdate::NotModified,
                computed: SourceUpdate::NotModified,
            },
            100,
        );

        assert!(complete);
        assert_eq!(cache.radar.unwrap().validated_at_unix, 100);
        assert_eq!(cache.computed.unwrap().validated_at_unix, 100);
        assert_eq!(cache.retry_count, 0);
    }

    #[test]
    fn not_modified_without_cache_is_a_failure() {
        let mut cache = RadarCache::default();

        let complete = apply_refresh(
            &mut cache,
            RadarRefreshResult {
                radar: SourceUpdate::NotModified,
                computed: SourceUpdate::NotModified,
            },
            100,
        );

        assert!(!complete);
        assert_eq!(cache.retry_count, 1);
    }

    #[test]
    fn expires_cached_sources_after_twenty_four_hours() {
        let mut cache = RadarCache {
            radar: Some(CachedSource {
                value: radar_recommendations("radar"),
                validated_at_unix: 100,
                validator: None,
            }),
            computed: Some(CachedSource {
                value: computed(),
                validated_at_unix: 101,
                validator: None,
            }),
            ..RadarCache::default()
        };

        cache.prune_stale(100 + CACHE_MAX_AGE_SECS + 1);

        assert!(cache.radar.is_none());
        assert!(cache.computed.is_some());
    }

    #[test]
    fn ignores_corrupt_and_unknown_cache_schemas() {
        assert!(parse_cache(b"not json", 100).is_none());
        assert!(parse_cache(br#"{"schema":2}"#, 100).is_none());
    }

    #[test]
    fn migrates_legacy_radar_recommendation_to_speed_slot() {
        let json = br#"{
            "schema": 1,
            "radar": {
                "value": {
                    "model": "legacy",
                    "effort": "high",
                    "iq": 95.0,
                    "average_cost_usd": 5.0,
                    "valid_tasks": 10
                },
                "validated_at_unix": 100
            }
        }"#;

        let cache = parse_cache(json, 100).unwrap();
        let radar = cache.radar.unwrap().value;

        assert_eq!(radar.speed.unwrap().model, "legacy");
        assert!(radar.smart.is_none());
    }

    #[test]
    fn round_trips_current_radar_cache() {
        let cache = RadarCache {
            radar: Some(CachedSource {
                value: RadarRecommendations {
                    speed: Some(recommendation("speed")),
                    smart: Some(recommendation("smart")),
                    daily_development: vec![recommendation("daily")],
                },
                validated_at_unix: 100,
                validator: Some("date".to_string()),
            }),
            ..RadarCache::default()
        };

        let json = serde_json::to_vec(&cache).unwrap();
        let parsed = parse_cache(&json, 100).unwrap().radar.unwrap().value;

        assert_eq!(parsed.speed.unwrap().model, "speed");
        assert_eq!(parsed.smart.unwrap().model, "smart");
        assert_eq!(parsed.daily_development[0].model, "daily");
    }

    #[test]
    fn schedules_success_and_retry_intervals() {
        let success = RadarCache {
            last_attempt_unix: Some(100),
            last_refresh_complete: true,
            ..RadarCache::default()
        };
        assert_eq!(next_due_in_secs(&success, 200), 60 * 60 - 100);

        for (retry_count, expected) in [
            (1, 15 * 60),
            (2, 30 * 60),
            (3, 60 * 60),
            (4, 6 * 60 * 60),
            (8, 6 * 60 * 60),
        ] {
            let failed = RadarCache {
                last_attempt_unix: Some(100),
                retry_count,
                last_refresh_complete: false,
                ..RadarCache::default()
            };
            assert_eq!(next_due_in_secs(&failed, 100), expected);
        }

        let interrupted = RadarCache {
            last_attempt_unix: Some(100),
            ..RadarCache::default()
        };
        assert_eq!(
            next_due_in_secs(&interrupted, 100),
            MANUAL_REFRESH_COOLDOWN_SECS
        );
    }
}
