use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

mod cache;
mod client;

#[cfg(test)]
pub(crate) use cache::CachedSource;
pub(crate) use cache::{
    apply_refresh, load_cache, next_due_in_secs, save_cache, RadarCache,
    MANUAL_REFRESH_COOLDOWN_SECS,
};
pub(crate) use client::{fetch_recommendations, FetchValidators, RadarRefreshResult, SourceUpdate};

pub(crate) const MIN_RECOMMENDED_IQ: f64 = 90.0;
const RADAR_INSIGHTS_SCHEMA: u32 = 1;
const EFFICIENCY_SCHEMA: u32 = 2;
const MAX_LABEL_CHARS: usize = 64;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Recommendation {
    pub(crate) model: String,
    pub(crate) effort: String,
    pub(crate) iq: f64,
    pub(crate) average_cost_usd: f64,
    pub(crate) valid_tasks: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RadarRecommendations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) speed: Option<Recommendation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) smart: Option<Recommendation>,
}

impl<'de> Deserialize<'de> for RadarRecommendations {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum WireFormat {
            Legacy(Recommendation),
            Current {
                #[serde(default)]
                speed: Option<Recommendation>,
                #[serde(default)]
                smart: Option<Recommendation>,
            },
        }

        Ok(match WireFormat::deserialize(deserializer)? {
            WireFormat::Legacy(recommendation) => Self {
                speed: Some(recommendation),
                smart: None,
            },
            WireFormat::Current { speed, smart } => Self { speed, smart },
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ComputedRecommendations {
    pub(crate) iq_per_dollar: Option<Recommendation>,
    pub(crate) intelligence_weighted: Option<Recommendation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RadarDataError {
    InvalidJson,
    UnsupportedSchema,
    InvalidData,
    RecommendationUnavailable,
}

impl fmt::Display for RadarDataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidJson => "invalid JSON",
            Self::UnsupportedSchema => "unsupported schema",
            Self::InvalidData => "invalid recommendation data",
            Self::RecommendationUnavailable => "recommendation unavailable",
        };
        formatter.write_str(message)
    }
}

#[derive(Deserialize)]
struct RadarInsightsResponse {
    schema: u32,
    recommendations: Vec<RadarRecommendationGroup>,
}

#[derive(Deserialize)]
struct RadarRecommendationGroup {
    key: String,
    items: Vec<RadarRecommendationItem>,
}

#[derive(Deserialize)]
struct RadarRecommendationItem {
    model: String,
    effort: String,
    iq: f64,
    average_cost_usd: f64,
    #[serde(default)]
    samples: u32,
    slot: Option<String>,
}

#[derive(Deserialize)]
struct EfficiencyResponse {
    schema: u32,
    points: Vec<EfficiencyPoint>,
}

#[derive(Deserialize)]
struct EfficiencyPoint {
    model: String,
    effort: String,
    iq: f64,
    average_price_usd: Option<f64>,
    #[serde(default)]
    valid_tasks: u32,
}

pub(crate) fn parse_radar_recommendation(
    json: &[u8],
) -> Result<RadarRecommendations, RadarDataError> {
    let response: RadarInsightsResponse =
        serde_json::from_slice(json).map_err(|_| RadarDataError::InvalidJson)?;
    if response.schema != RADAR_INSIGHTS_SCHEMA {
        return Err(RadarDataError::UnsupportedSchema);
    }

    let items = response
        .recommendations
        .into_iter()
        .find(|group| group.key == "daily_development")
        .map(|group| group.items)
        .ok_or(RadarDataError::RecommendationUnavailable)?;

    let mut speed = None;
    let mut smart = None;
    let mut legacy_value = None;
    for item in items {
        let destination = match item.slot.as_deref() {
            Some("speed") if speed.is_none() => &mut speed,
            Some("smart") if smart.is_none() => &mut smart,
            Some("value") if legacy_value.is_none() => &mut legacy_value,
            _ => continue,
        };
        *destination = Some(
            recommendation_from_parts(
                item.model,
                item.effort,
                item.iq,
                item.average_cost_usd,
                item.samples,
            )
            .ok_or(RadarDataError::InvalidData)?,
        );
    }

    if speed.is_none() {
        speed = legacy_value;
    }
    if speed.is_none() && smart.is_none() {
        return Err(RadarDataError::RecommendationUnavailable);
    }

    Ok(RadarRecommendations { speed, smart })
}

pub(crate) fn parse_efficiency_recommendations(
    json: &[u8],
) -> Result<ComputedRecommendations, RadarDataError> {
    let response: EfficiencyResponse =
        serde_json::from_slice(json).map_err(|_| RadarDataError::InvalidJson)?;
    if response.schema != EFFICIENCY_SCHEMA {
        return Err(RadarDataError::UnsupportedSchema);
    }

    let mut valid_points = 0usize;
    let candidates: Vec<Recommendation> = response
        .points
        .into_iter()
        .filter_map(|point| {
            let model = sanitize_label(&point.model)?;
            let effort = sanitize_label(&point.effort)?;
            if !point.iq.is_finite() {
                return None;
            }

            let cost = point.average_price_usd?;
            if !cost.is_finite() || cost <= 0.0 {
                return None;
            }
            valid_points += 1;
            if point.iq < MIN_RECOMMENDED_IQ {
                return None;
            }

            Some(Recommendation {
                model,
                effort,
                iq: point.iq,
                average_cost_usd: cost,
                valid_tasks: point.valid_tasks,
            })
        })
        .collect();

    if valid_points == 0 {
        return Err(RadarDataError::InvalidData);
    }

    Ok(rank_candidates(&candidates))
}

pub(crate) fn model_display_name(model: &str) -> String {
    match model {
        "gpt-5.6-sol" => "Sol".to_string(),
        "gpt-5.6-terra" => "Terra".to_string(),
        "gpt-5.6-luna" => "Luna".to_string(),
        "gpt-5.5" => "GPT-5.5".to_string(),
        value => sanitize_label(value).unwrap_or_else(|| "Unknown".to_string()),
    }
}

fn recommendation_from_parts(
    model: String,
    effort: String,
    iq: f64,
    average_cost_usd: f64,
    valid_tasks: u32,
) -> Option<Recommendation> {
    if !iq.is_finite() || !average_cost_usd.is_finite() || average_cost_usd <= 0.0 {
        return None;
    }
    Some(Recommendation {
        model: sanitize_label(&model)?,
        effort: sanitize_label(&effort)?,
        iq,
        average_cost_usd,
        valid_tasks,
    })
}

fn sanitize_label(value: &str) -> Option<String> {
    let label: String = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_LABEL_CHARS)
        .collect();
    (!label.is_empty()).then_some(label)
}

fn rank_candidates(candidates: &[Recommendation]) -> ComputedRecommendations {
    if candidates.is_empty() {
        return ComputedRecommendations {
            iq_per_dollar: None,
            intelligence_weighted: None,
        };
    }

    let maximum_iq = candidates
        .iter()
        .map(|candidate| candidate.iq)
        .fold(f64::NEG_INFINITY, f64::max);
    let minimum_cost = candidates
        .iter()
        .map(|candidate| candidate.average_cost_usd)
        .fold(f64::INFINITY, f64::min);

    ComputedRecommendations {
        iq_per_dollar: select_best(candidates, |candidate| {
            candidate.iq / candidate.average_cost_usd
        }),
        intelligence_weighted: select_best(candidates, |candidate| {
            let intelligence_score = candidate.iq / maximum_iq;
            let cost_score = minimum_cost / candidate.average_cost_usd;
            0.8 * intelligence_score + 0.2 * cost_score
        }),
    }
}

fn select_best(
    candidates: &[Recommendation],
    score: impl Fn(&Recommendation) -> f64,
) -> Option<Recommendation> {
    candidates
        .iter()
        .max_by(|left, right| compare_candidates(left, right, score(left), score(right)))
        .cloned()
}

fn compare_candidates(
    left: &Recommendation,
    right: &Recommendation,
    left_score: f64,
    right_score: f64,
) -> Ordering {
    left_score
        .total_cmp(&right_score)
        .then_with(|| left.iq.total_cmp(&right.iq))
        .then_with(|| right.average_cost_usd.total_cmp(&left.average_cost_usd))
        .then_with(|| left.valid_tasks.cmp(&right.valid_tasks))
        .then_with(|| right.model.cmp(&left.model))
        .then_with(|| right.effort.cmp(&left.effort))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_current_daily_development_recommendations() {
        let json = br#"{
            "schema": 1,
            "extra": true,
            "recommendations": [
                {
                    "key": "hard_problems",
                    "items": [{
                        "model": "gpt-5.6-sol",
                        "effort": "max",
                        "iq": 103.1,
                        "average_cost_usd": 9.04,
                        "slot": "value"
                    }]
                },
                {
                    "key": "daily_development",
                    "items": [
                        {
                            "model": "gpt-5.6-sol",
                            "effort": "medium",
                            "iq": 91.07,
                            "average_cost_usd": 3.742,
                            "samples": 112,
                            "slot": "speed",
                            "ignored": "field"
                        },
                        {
                            "model": "gpt-5.6-sol",
                            "effort": "xhigh",
                            "iq": 105.8,
                            "average_cost_usd": 6.19,
                            "slot": "smart"
                        }
                    ]
                }
            ]
        }"#;

        let recommendations = parse_radar_recommendation(json).unwrap();

        let speed = recommendations.speed.unwrap();
        assert_eq!(speed.model, "gpt-5.6-sol");
        assert_eq!(speed.effort, "medium");
        assert_eq!(speed.valid_tasks, 112);
        assert_eq!(recommendations.smart.unwrap().effort, "xhigh");
    }

    #[test]
    fn falls_back_to_legacy_value_for_speed() {
        let json = br#"{
            "schema": 1,
            "recommendations": [{
                "key": "daily_development",
                "items": [{
                    "model": "gpt-5.6-sol",
                    "effort": "high",
                    "iq": 89.73,
                    "average_cost_usd": 4.998,
                    "samples": 112,
                    "slot": "value"
                }]
            }]
        }"#;

        let recommendations = parse_radar_recommendation(json).unwrap();

        assert_eq!(recommendations.speed.unwrap().effort, "high");
        assert!(recommendations.smart.is_none());
    }

    #[test]
    fn accepts_a_single_current_radar_slot() {
        let json = br#"{
            "schema": 1,
            "recommendations": [{
                "key": "daily_development",
                "items": [{
                    "model": "gpt-5.5",
                    "effort": "xhigh",
                    "iq": 100.45,
                    "average_cost_usd": 5.737,
                    "slot": "smart"
                }]
            }]
        }"#;

        let recommendations = parse_radar_recommendation(json).unwrap();

        assert!(recommendations.speed.is_none());
        assert_eq!(recommendations.smart.unwrap().model, "gpt-5.5");
    }

    #[test]
    fn rejects_unsupported_or_incomplete_radar_data() {
        let unsupported = br#"{"schema":2,"recommendations":[]}"#;
        assert_eq!(
            parse_radar_recommendation(unsupported),
            Err(RadarDataError::UnsupportedSchema)
        );

        let missing_supported_slot =
            br#"{"schema":1,"recommendations":[{"key":"daily_development","items":[]}]}"#;
        assert_eq!(
            parse_radar_recommendation(missing_supported_slot),
            Err(RadarDataError::RecommendationUnavailable)
        );
    }

    #[test]
    fn ranks_iq_per_dollar_and_intelligence_weighted_independently() {
        let json = br#"{
            "schema": 2,
            "points": [
                {"model":"gpt-5.6-sol","effort":"high","iq":89.999,"average_price_usd":1.0,"valid_tasks":112},
                {"model":"gpt-5.6-sol","effort":"xhigh","iq":105.8,"average_price_usd":6.19,"valid_tasks":112},
                {"model":"gpt-5.6-sol","effort":"max","iq":103.1,"average_price_usd":9.04,"valid_tasks":112},
                {"model":"gpt-5.6-terra","effort":"max","iq":93.75,"average_price_usd":4.70,"valid_tasks":112}
            ]
        }"#;

        let recommendations = parse_efficiency_recommendations(json).unwrap();

        assert_eq!(
            recommendations.iq_per_dollar.unwrap().model,
            "gpt-5.6-terra"
        );
        let weighted = recommendations.intelligence_weighted.unwrap();
        assert_eq!(weighted.model, "gpt-5.6-sol");
        assert_eq!(weighted.effort, "xhigh");
    }

    #[test]
    fn includes_iq_equal_to_ninety_and_excludes_lower_values() {
        let json = br#"{
            "schema": 2,
            "points": [
                {"model":"below","effort":"high","iq":89.999,"average_price_usd":0.1,"valid_tasks":10},
                {"model":"boundary","effort":"high","iq":90.0,"average_price_usd":5.0,"valid_tasks":10}
            ]
        }"#;

        let recommendations = parse_efficiency_recommendations(json).unwrap();

        assert_eq!(recommendations.iq_per_dollar.unwrap().model, "boundary");
        assert_eq!(
            recommendations.intelligence_weighted.unwrap().model,
            "boundary"
        );
    }

    #[test]
    fn uses_deterministic_tie_breaking() {
        let json = br#"{
            "schema": 2,
            "points": [
                {"model":"beta","effort":"high","iq":90.0,"average_price_usd":5.0,"valid_tasks":10},
                {"model":"alpha","effort":"high","iq":90.0,"average_price_usd":5.0,"valid_tasks":10}
            ]
        }"#;

        let recommendations = parse_efficiency_recommendations(json).unwrap();

        assert_eq!(recommendations.iq_per_dollar.unwrap().model, "alpha");
        assert_eq!(
            recommendations.intelligence_weighted.unwrap().model,
            "alpha"
        );
    }

    #[test]
    fn reports_no_recommendation_when_valid_points_do_not_qualify() {
        let json = br#"{
            "schema": 2,
            "points": [
                {"model":"gpt-5.6-luna","effort":"low","iq":4.0,"average_price_usd":0.2,"valid_tasks":112}
            ]
        }"#;

        let recommendations = parse_efficiency_recommendations(json).unwrap();

        assert!(recommendations.iq_per_dollar.is_none());
        assert!(recommendations.intelligence_weighted.is_none());
    }

    #[test]
    fn rejects_efficiency_payloads_without_valid_points() {
        let json =
            br#"{"schema":2,"points":[{"model":"","effort":"high","iq":90,"average_price_usd":5}]}"#;

        assert_eq!(
            parse_efficiency_recommendations(json),
            Err(RadarDataError::InvalidData)
        );
    }

    #[test]
    fn rejects_efficiency_payloads_without_valid_prices() {
        let json = br#"{
            "schema": 2,
            "points": [
                {"model":"missing","effort":"high","iq":90},
                {"model":"zero","effort":"high","iq":90,"average_price_usd":0},
                {"model":"negative","effort":"high","iq":90,"average_price_usd":-1}
            ]
        }"#;

        assert_eq!(
            parse_efficiency_recommendations(json),
            Err(RadarDataError::InvalidData)
        );
    }

    #[test]
    fn maps_known_models_and_sanitizes_unknown_labels() {
        assert_eq!(model_display_name("gpt-5.6-sol"), "Sol");
        assert_eq!(model_display_name(" custom\u{0007}model "), "custommodel");
    }
}
