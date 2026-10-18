use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{ComputedRecommendations, FetchValidators, SourceUpdate};

const CACHE_SCHEMA: u32 = 2;
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
        self.computed.is_some()
    }

    pub(crate) fn validators(&self) -> FetchValidators {
        FetchValidators {
            efficiency_etag: self
                .computed
                .as_ref()
                .and_then(|source| source.validator.clone()),
        }
    }

    pub(crate) fn prune_stale(&mut self, now_unix: u64) {
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
    refresh: SourceUpdate<ComputedRecommendations>,
    now_unix: u64,
) -> bool {
    let complete = apply_source(&mut cache.computed, refresh, now_unix);

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
    if cache.schema == 1 {
        cache.schema = CACHE_SCHEMA;
        cache.last_attempt_unix = None;
        cache.last_complete_success_unix = None;
        cache.retry_count = 0;
        cache.last_refresh_complete = false;
    } else if cache.schema != CACHE_SCHEMA {
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
            average_minutes: None,
            passed_tasks: None,
            valid_tasks: 10,
        }
    }

    fn computed() -> ComputedRecommendations {
        ComputedRecommendations {
            daily: Some(recommendation("daily")),
            hard_problem: Some(recommendation("weighted")),
            radar_bench: false,
        }
    }

    #[test]
    fn preserves_recommendations_when_refresh_fails() {
        let mut cache = RadarCache {
            computed: Some(CachedSource {
                value: computed(),
                validated_at_unix: 10,
                validator: None,
            }),
            ..RadarCache::default()
        };
        let complete = apply_refresh(&mut cache, SourceUpdate::Failed("offline".to_string()), 100);

        assert!(!complete);
        assert_eq!(cache.computed.as_ref().unwrap().value, computed());
        assert_eq!(cache.computed.as_ref().unwrap().validated_at_unix, 10);
        assert_eq!(cache.retry_count, 1);
        assert!(!cache.last_refresh_complete);
    }

    #[test]
    fn not_modified_refreshes_existing_cache_age() {
        let mut cache = RadarCache {
            computed: Some(CachedSource {
                value: computed(),
                validated_at_unix: 10,
                validator: Some("etag".to_string()),
            }),
            ..RadarCache::default()
        };

        let complete = apply_refresh(&mut cache, SourceUpdate::NotModified, 100);

        assert!(complete);
        assert_eq!(cache.computed.unwrap().validated_at_unix, 100);
        assert_eq!(cache.retry_count, 0);
    }

    #[test]
    fn not_modified_without_cache_is_a_failure() {
        let mut cache = RadarCache::default();

        let complete = apply_refresh(&mut cache, SourceUpdate::NotModified, 100);

        assert!(!complete);
        assert_eq!(cache.retry_count, 1);
    }

    #[test]
    fn expires_cached_sources_after_twenty_four_hours() {
        let mut cache = RadarCache {
            computed: Some(CachedSource {
                value: computed(),
                validated_at_unix: 101,
                validator: None,
            }),
            ..RadarCache::default()
        };

        cache.prune_stale(100 + CACHE_MAX_AGE_SECS + 1);

        assert!(cache.computed.is_some());
        cache.prune_stale(101 + CACHE_MAX_AGE_SECS + 1);
        assert!(cache.computed.is_none());
    }

    #[test]
    fn ignores_corrupt_and_unknown_cache_schemas() {
        assert!(parse_cache(b"not json", 100).is_none());
        assert!(parse_cache(br#"{"schema":3}"#, 100).is_none());
    }

    #[test]
    fn migrates_legacy_cache_and_clears_site_failure_backoff() {
        let json = serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "radar": { "value": { "speed": recommendation("site") }, "validated_at_unix": 100 },
            "computed": { "value": computed(), "validated_at_unix": 100 },
            "last_attempt_unix": 100,
            "retry_count": 37,
            "last_refresh_complete": false
        }))
        .unwrap();

        let cache = parse_cache(&json, 101).unwrap();

        assert_eq!(cache.schema, CACHE_SCHEMA);
        assert_eq!(cache.computed.as_ref().unwrap().value, computed());
        assert_eq!(cache.retry_count, 0);
        assert_eq!(next_due_in_secs(&cache, 101), 0);
        assert!(serde_json::to_value(&cache).unwrap().get("radar").is_none());
    }

    #[test]
    fn keeps_old_computed_cache_without_inventing_daily_metrics() {
        let json = br#"{
            "schema": 1,
            "computed": {
                "value": {
                    "intelligence_weighted": {
                        "model": "legacy",
                        "effort": "high",
                        "iq": 95.0,
                        "average_cost_usd": 5.0,
                        "valid_tasks": 10
                    }
                },
                "validated_at_unix": 100
            }
        }"#;

        let cache = parse_cache(json, 100).unwrap();
        let computed = cache.computed.unwrap().value;

        assert!(computed.daily.is_none());
        assert!(computed.hard_problem.is_none());
    }

    #[test]
    fn round_trips_current_recommendation_cache() {
        let cache = RadarCache {
            computed: Some(CachedSource {
                value: computed(),
                validated_at_unix: 100,
                validator: Some("date".to_string()),
            }),
            ..RadarCache::default()
        };

        let json = serde_json::to_vec(&cache).unwrap();
        let parsed = parse_cache(&json, 100).unwrap().computed.unwrap().value;

        assert_eq!(parsed, computed());
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
