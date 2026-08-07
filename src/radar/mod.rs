use std::cmp::Ordering;
use std::collections::HashMap;
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
const EFFICIENCY_SCHEMA: u32 = 1;
const MAX_LABEL_CHARS: usize = 64;
const DAILY_PRICE_WEIGHT: f64 = 0.7;
const DAILY_TIME_WEIGHT: f64 = 0.3;
const DAILY_PRICE_REFERENCE_USD: f64 = 1.0;
const DAILY_TIME_REFERENCE_MINUTES: f64 = 10.0;
const HARD_TARGET_PRICE_USD: f64 = 8.0;
const HARD_TARGET_MINUTES: f64 = 30.0;
const HARD_MAX_PRICE_USD: f64 = 9.0;
const HARD_MAX_MINUTES: f64 = 30.0;
const HARD_IQ_GAP: f64 = 10.0;
const WILSON_Z_SCORE: f64 = 1.959_963_984_540_054;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Recommendation {
    pub(crate) model: String,
    pub(crate) effort: String,
    pub(crate) iq: f64,
    pub(crate) average_cost_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) average_minutes: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) passed_tasks: Option<u32>,
    pub(crate) valid_tasks: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RadarRecommendations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) speed: Option<Recommendation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) smart: Option<Recommendation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) daily_development: Vec<Recommendation>,
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
                #[serde(default)]
                daily_development: Vec<Recommendation>,
            },
        }

        Ok(match WireFormat::deserialize(deserializer)? {
            WireFormat::Legacy(recommendation) => Self {
                speed: Some(recommendation),
                smart: None,
                daily_development: Vec::new(),
            },
            WireFormat::Current {
                speed,
                smart,
                daily_development,
            } => Self {
                speed,
                smart,
                daily_development,
            },
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ComputedRecommendations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) daily: Option<Recommendation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hard_problem: Option<Recommendation>,
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
    combos: Vec<EfficiencyCombo>,
    tasks: Vec<EfficiencyTask>,
    cells: HashMap<String, EfficiencyCell>,
}

#[derive(Deserialize)]
struct EfficiencyCombo {
    model: String,
    effort: String,
}

#[derive(Deserialize)]
struct EfficiencyTask {
    id: String,
}

#[derive(Deserialize)]
struct EfficiencyCell {
    #[serde(default)]
    ran_by: Vec<EfficiencyRun>,
}

#[derive(Deserialize)]
struct EfficiencyRun {
    passed: Option<bool>,
    duration_sec: Option<f64>,
    actual_cost_usd: Option<f64>,
    cost_complete: Option<bool>,
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
    let mut daily_development = Vec::new();
    let parse_item = |item: RadarRecommendationItem| {
        recommendation_from_parts(
            item.model,
            item.effort,
            item.iq,
            item.average_cost_usd,
            item.samples,
        )
        .ok_or(RadarDataError::InvalidData)
    };
    for item in items {
        let destination = match item.slot.as_deref() {
            Some("speed") if speed.is_none() => &mut speed,
            Some("smart") if smart.is_none() => &mut smart,
            Some("value") if legacy_value.is_none() => &mut legacy_value,
            None => {
                daily_development.push(parse_item(item)?);
                continue;
            }
            _ => continue,
        };
        *destination = Some(parse_item(item)?);
    }

    if speed.is_none() {
        speed = legacy_value;
    }
    if speed.is_none() && smart.is_none() && daily_development.is_empty() {
        return Err(RadarDataError::RecommendationUnavailable);
    }

    Ok(RadarRecommendations {
        speed,
        smart,
        daily_development,
    })
}

pub(crate) fn parse_efficiency_recommendations(
    json: &[u8],
) -> Result<ComputedRecommendations, RadarDataError> {
    let response: EfficiencyResponse =
        serde_json::from_slice(json).map_err(|_| RadarDataError::InvalidJson)?;
    if response.schema != EFFICIENCY_SCHEMA {
        return Err(RadarDataError::UnsupportedSchema);
    }

    let mut complete_points = 0usize;
    let mut candidates = Vec::new();
    for combo in &response.combos {
        let Some(model) = sanitize_label(&combo.model) else {
            continue;
        };
        let Some(effort) = sanitize_label(&combo.effort) else {
            continue;
        };
        let Some(point) = aggregate_efficiency_point(&response, combo, &effort) else {
            continue;
        };
        let Some(average_cost_usd) = point.average_cost_usd else {
            continue;
        };
        let Some(average_minutes) = point.average_minutes else {
            continue;
        };
        if !average_cost_usd.is_finite()
            || average_cost_usd <= 0.0
            || !average_minutes.is_finite()
            || average_minutes <= 0.0
        {
            continue;
        }
        complete_points += 1;
        if point.iq < MIN_RECOMMENDED_IQ {
            continue;
        }

        candidates.push(Recommendation {
            model,
            effort,
            iq: point.iq,
            average_cost_usd,
            average_minutes: Some(average_minutes),
            passed_tasks: Some(point.passed_tasks),
            valid_tasks: point.valid_tasks,
        });
    }

    if complete_points == 0 {
        return Err(RadarDataError::InvalidData);
    }

    Ok(rank_candidates(&candidates))
}

struct AggregatedEfficiencyPoint {
    iq: f64,
    passed_tasks: u32,
    valid_tasks: u32,
    average_cost_usd: Option<f64>,
    average_minutes: Option<f64>,
}

fn aggregate_efficiency_point(
    response: &EfficiencyResponse,
    combo: &EfficiencyCombo,
    effort: &str,
) -> Option<AggregatedEfficiencyPoint> {
    let mut passed_tasks = 0u32;
    let mut valid_tasks = 0u32;
    let mut cost_sum = 0.0;
    let mut cost_samples = 0u32;
    let mut duration_sum_secs = 0.0;
    let mut duration_samples = 0u32;

    for task in &response.tasks {
        let cell_key = format!("{}|{}|{}", task.id, combo.model, combo.effort);
        let Some(cell) = response.cells.get(&cell_key) else {
            continue;
        };
        let Some(run) = cell.ran_by.first() else {
            continue;
        };
        let Some(passed) = run.passed else {
            continue;
        };
        valid_tasks = valid_tasks.saturating_add(1);
        if passed {
            passed_tasks = passed_tasks.saturating_add(1);
        }

        if let Some(duration_sec) = run.duration_sec {
            if duration_sec.is_finite() && duration_sec > 0.0 {
                duration_sum_secs += duration_sec;
                duration_samples = duration_samples.saturating_add(1);
            }
        }

        if let Some(actual_cost_usd) = run.actual_cost_usd {
            if actual_cost_usd.is_finite()
                && actual_cost_usd > 0.0
                && (effort != "ultra" || run.cost_complete == Some(true))
            {
                cost_sum += actual_cost_usd;
                cost_samples = cost_samples.saturating_add(1);
            }
        }
    }

    if valid_tasks == 0 {
        return None;
    }

    Some(AggregatedEfficiencyPoint {
        iq: f64::from(passed_tasks) / f64::from(valid_tasks) * 150.0,
        passed_tasks,
        valid_tasks,
        average_cost_usd: (cost_samples > 0).then(|| cost_sum / f64::from(cost_samples)),
        average_minutes: (duration_samples > 0)
            .then(|| duration_sum_secs / f64::from(duration_samples) / 60.0),
    })
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
        average_minutes: None,
        passed_tasks: None,
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
            daily: None,
            hard_problem: None,
        };
    }

    ComputedRecommendations {
        daily: select_daily(candidates),
        hard_problem: select_hard(candidates),
    }
}

fn select_daily(candidates: &[Recommendation]) -> Option<Recommendation> {
    candidates
        .iter()
        .filter_map(|candidate| Some((candidate, daily_cost(candidate)?)))
        .max_by(|(left, left_cost), (right, right_cost)| {
            right_cost
                .total_cmp(left_cost)
                .then_with(|| credible_iq(left).total_cmp(&credible_iq(right)))
                .then_with(|| left.iq.total_cmp(&right.iq))
                .then_with(|| right.average_cost_usd.total_cmp(&left.average_cost_usd))
                .then_with(|| {
                    right
                        .average_minutes
                        .unwrap_or(f64::INFINITY)
                        .total_cmp(&left.average_minutes.unwrap_or(f64::INFINITY))
                })
                .then_with(|| left.valid_tasks.cmp(&right.valid_tasks))
                .then_with(|| right.model.cmp(&left.model))
                .then_with(|| right.effort.cmp(&left.effort))
        })
        .map(|(candidate, _)| candidate.clone())
}

fn daily_cost(candidate: &Recommendation) -> Option<f64> {
    weighted_cost(
        candidate,
        DAILY_PRICE_REFERENCE_USD,
        DAILY_TIME_REFERENCE_MINUTES,
    )
}

fn hard_cost(candidate: &Recommendation) -> Option<f64> {
    weighted_cost(candidate, HARD_TARGET_PRICE_USD, HARD_TARGET_MINUTES)
}

fn weighted_cost(
    candidate: &Recommendation,
    price_reference_usd: f64,
    time_reference_minutes: f64,
) -> Option<f64> {
    let minutes = candidate.average_minutes?;
    let score = (candidate.average_cost_usd / price_reference_usd).powf(DAILY_PRICE_WEIGHT)
        * (minutes / time_reference_minutes).powf(DAILY_TIME_WEIGHT);
    score.is_finite().then_some(score)
}

fn select_hard(candidates: &[Recommendation]) -> Option<Recommendation> {
    let eligible = candidates
        .iter()
        .filter(|candidate| {
            candidate.average_cost_usd <= HARD_MAX_PRICE_USD
                && candidate
                    .average_minutes
                    .is_some_and(|minutes| minutes <= HARD_MAX_MINUTES)
        })
        .collect::<Vec<_>>();
    let cost_leader = eligible
        .iter()
        .filter_map(|candidate| hard_cost(candidate).map(|cost| (*candidate, cost)))
        .max_by(|(left, left_cost), (right, right_cost)| {
            right_cost
                .total_cmp(left_cost)
                .then_with(|| credible_iq(left).total_cmp(&credible_iq(right)))
                .then_with(|| compare_hard_ties(left, right))
        })
        .map(|(candidate, _)| candidate)?;
    let cost_leader_iq = credible_iq(cost_leader);
    let high_iq_candidates = eligible
        .iter()
        .copied()
        .filter(|candidate| credible_iq(candidate) >= cost_leader_iq + HARD_IQ_GAP)
        .collect::<Vec<_>>();

    if high_iq_candidates.is_empty() {
        eligible
            .iter()
            .filter_map(|candidate| hard_cost(candidate).map(|cost| (*candidate, cost)))
            .max_by(|(left, left_cost), (right, right_cost)| {
                right_cost
                    .total_cmp(left_cost)
                    .then_with(|| credible_iq(left).total_cmp(&credible_iq(right)))
                    .then_with(|| compare_hard_ties(left, right))
            })
            .map(|(candidate, _)| candidate.clone())
    } else {
        high_iq_candidates
            .into_iter()
            .max_by(compare_hard_iq)
            .cloned()
    }
}

fn compare_hard_iq(left: &&Recommendation, right: &&Recommendation) -> Ordering {
    credible_iq(left)
        .total_cmp(&credible_iq(right))
        .then_with(|| compare_hard_ties(left, right))
}

fn compare_hard_ties(left: &Recommendation, right: &Recommendation) -> Ordering {
    right
        .average_cost_usd
        .total_cmp(&left.average_cost_usd)
        .then_with(|| {
            right
                .average_minutes
                .unwrap_or(f64::INFINITY)
                .total_cmp(&left.average_minutes.unwrap_or(f64::INFINITY))
        })
        .then_with(|| left.valid_tasks.cmp(&right.valid_tasks))
        .then_with(|| right.model.cmp(&left.model))
        .then_with(|| right.effort.cmp(&left.effort))
}

fn credible_iq(candidate: &Recommendation) -> f64 {
    let Some(passed_tasks) = candidate.passed_tasks else {
        return candidate.iq;
    };
    if candidate.valid_tasks == 0 || passed_tasks > candidate.valid_tasks {
        return candidate.iq;
    }

    let sample_size = f64::from(candidate.valid_tasks);
    let pass_rate = f64::from(passed_tasks) / sample_size;
    let z_squared = WILSON_Z_SCORE * WILSON_Z_SCORE;
    let denominator = 1.0 + z_squared / sample_size;
    let center = (pass_rate + z_squared / (2.0 * sample_size)) / denominator;
    let margin = WILSON_Z_SCORE
        * (pass_rate * (1.0 - pass_rate) / sample_size
            + z_squared / (4.0 * sample_size * sample_size))
            .sqrt()
        / denominator;
    (center - margin).max(0.0) * 150.0
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
    fn keeps_untagged_daily_development_recommendations_unlabeled() {
        let json = br#"{
            "schema": 1,
            "recommendations": [{
                "key": "daily_development",
                "items": [
                    {
                        "model": "gpt-5.5",
                        "effort": "high",
                        "iq": 95.09,
                        "average_cost_usd": 3.663656,
                        "samples": 112
                    },
                    {
                        "model": "gpt-5.6-luna",
                        "effort": "max",
                        "iq": 95.09,
                        "average_cost_usd": 0.47011,
                        "samples": 112
                    }
                ]
            }]
        }"#;

        let recommendations = parse_radar_recommendation(json).unwrap();

        assert!(recommendations.speed.is_none());
        assert!(recommendations.smart.is_none());
        assert_eq!(recommendations.daily_development.len(), 2);
        assert_eq!(recommendations.daily_development[0].model, "gpt-5.5");
        assert_eq!(recommendations.daily_development[1].model, "gpt-5.6-luna");
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

    struct FixturePoint<'a> {
        model: &'a str,
        effort: &'a str,
        passed_tasks: u32,
        valid_tasks: u32,
        cost_usd: Option<f64>,
        minutes: Option<f64>,
        cost_complete: Option<bool>,
    }

    fn efficiency_payload(points: &[FixturePoint<'_>]) -> Vec<u8> {
        let task_count = points
            .iter()
            .map(|point| point.valid_tasks)
            .max()
            .unwrap_or_default();
        let tasks = (0..task_count)
            .map(|index| serde_json::json!({ "id": format!("task-{index}") }))
            .collect::<Vec<_>>();
        let combos = points
            .iter()
            .map(|point| serde_json::json!({ "model": point.model, "effort": point.effort }))
            .collect::<Vec<_>>();
        let mut cells = serde_json::Map::new();

        for point in points {
            for task_index in 0..point.valid_tasks {
                let passed = task_index < point.passed_tasks;
                let mut run = serde_json::Map::new();
                run.insert("passed".to_string(), serde_json::json!(passed));
                if let Some(minutes) = point.minutes {
                    run.insert(
                        "duration_sec".to_string(),
                        serde_json::json!(minutes * 60.0),
                    );
                }
                if let Some(cost_usd) = point.cost_usd {
                    run.insert("actual_cost_usd".to_string(), serde_json::json!(cost_usd));
                }
                if let Some(cost_complete) = point.cost_complete {
                    run.insert(
                        "cost_complete".to_string(),
                        serde_json::json!(cost_complete),
                    );
                }
                let key = format!("task-{task_index}|{}|{}", point.model, point.effort);
                cells.insert(
                    key,
                    serde_json::json!({ "ran_by": [serde_json::Value::Object(run)] }),
                );
            }
        }

        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "combos": combos,
            "tasks": tasks,
            "cells": cells,
        }))
        .unwrap()
    }

    #[test]
    fn aggregates_raw_efficiency_cells_and_ignores_incomplete_ultra_costs() {
        let json = efficiency_payload(&[
            FixturePoint {
                model: "gpt-5.6-sol",
                effort: "high",
                passed_tasks: 3,
                valid_tasks: 4,
                cost_usd: Some(2.0),
                minutes: Some(2.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "gpt-5.6-luna",
                effort: "ultra",
                passed_tasks: 4,
                valid_tasks: 4,
                cost_usd: Some(3.0),
                minutes: Some(3.0),
                cost_complete: Some(false),
            },
        ]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();
        let daily = recommendations.daily.unwrap();

        assert_eq!(daily.model, "gpt-5.6-sol");
        assert_eq!(daily.iq, 112.5);
        assert_eq!(daily.average_cost_usd, 2.0);
        assert_eq!(daily.average_minutes, Some(2.0));
        assert_eq!(daily.passed_tasks, Some(3));
        assert_eq!(daily.valid_tasks, 4);
    }

    #[test]
    fn keeps_iq_ninety_boundary_and_excludes_lower_values() {
        let json = efficiency_payload(&[
            FixturePoint {
                model: "below",
                effort: "high",
                passed_tasks: 5,
                valid_tasks: 10,
                cost_usd: Some(0.1),
                minutes: Some(1.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "boundary",
                effort: "high",
                passed_tasks: 6,
                valid_tasks: 10,
                cost_usd: Some(5.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
        ]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();

        assert_eq!(recommendations.daily.unwrap().model, "boundary");
        assert_eq!(recommendations.hard_problem.unwrap().model, "boundary");
    }

    #[test]
    fn daily_recommendation_weights_price_more_than_time() {
        let json = efficiency_payload(&[
            FixturePoint {
                model: "cheap-slow",
                effort: "high",
                passed_tasks: 6,
                valid_tasks: 10,
                cost_usd: Some(1.0),
                minutes: Some(30.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "expensive-fast",
                effort: "high",
                passed_tasks: 10,
                valid_tasks: 10,
                cost_usd: Some(2.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
        ]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();

        assert_eq!(recommendations.daily.unwrap().model, "cheap-slow");
    }

    #[test]
    fn uses_deterministic_tie_breaking() {
        let json = efficiency_payload(&[
            FixturePoint {
                model: "beta",
                effort: "high",
                passed_tasks: 9,
                valid_tasks: 10,
                cost_usd: Some(5.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "alpha",
                effort: "high",
                passed_tasks: 9,
                valid_tasks: 10,
                cost_usd: Some(5.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
        ]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();

        assert_eq!(recommendations.daily.unwrap().model, "alpha");
        assert_eq!(recommendations.hard_problem.unwrap().model, "alpha");
    }

    #[test]
    fn hard_recommendation_prefers_cost_when_credible_iq_gap_is_small() {
        let json = efficiency_payload(&[
            FixturePoint {
                model: "low-cost",
                effort: "high",
                passed_tasks: 60,
                valid_tasks: 100,
                cost_usd: Some(1.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "higher-iq",
                effort: "high",
                passed_tasks: 63,
                valid_tasks: 100,
                cost_usd: Some(8.0),
                minutes: Some(30.0),
                cost_complete: None,
            },
        ]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();

        assert_eq!(recommendations.hard_problem.unwrap().model, "low-cost");
    }

    #[test]
    fn hard_recommendation_prefers_higher_iq_when_gap_reaches_ten_points() {
        let json = efficiency_payload(&[
            FixturePoint {
                model: "low-cost",
                effort: "high",
                passed_tasks: 60,
                valid_tasks: 100,
                cost_usd: Some(1.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "higher-iq",
                effort: "high",
                passed_tasks: 70,
                valid_tasks: 100,
                cost_usd: Some(8.0),
                minutes: Some(30.0),
                cost_complete: None,
            },
        ]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();

        assert_eq!(recommendations.hard_problem.unwrap().model, "higher-iq");
    }

    #[test]
    fn hard_recommendation_excludes_price_and_time_overrides() {
        let json = efficiency_payload(&[
            FixturePoint {
                model: "acceptable",
                effort: "high",
                passed_tasks: 60,
                valid_tasks: 100,
                cost_usd: Some(1.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "too-expensive",
                effort: "high",
                passed_tasks: 100,
                valid_tasks: 100,
                cost_usd: Some(12.0),
                minutes: Some(10.0),
                cost_complete: None,
            },
            FixturePoint {
                model: "too-slow",
                effort: "high",
                passed_tasks: 100,
                valid_tasks: 100,
                cost_usd: Some(8.0),
                minutes: Some(40.0),
                cost_complete: None,
            },
        ]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();

        assert_eq!(recommendations.hard_problem.unwrap().model, "acceptable");
    }

    #[test]
    fn reports_no_recommendation_when_valid_points_do_not_qualify() {
        let json = efficiency_payload(&[FixturePoint {
            model: "gpt-5.6-luna",
            effort: "low",
            passed_tasks: 4,
            valid_tasks: 10,
            cost_usd: Some(0.2),
            minutes: Some(1.0),
            cost_complete: None,
        }]);

        let recommendations = parse_efficiency_recommendations(&json).unwrap();

        assert!(recommendations.daily.is_none());
        assert!(recommendations.hard_problem.is_none());
    }

    #[test]
    fn rejects_efficiency_payloads_without_complete_points() {
        let json = efficiency_payload(&[FixturePoint {
            model: "missing-cost",
            effort: "high",
            passed_tasks: 10,
            valid_tasks: 10,
            cost_usd: None,
            minutes: Some(10.0),
            cost_complete: None,
        }]);

        assert_eq!(
            parse_efficiency_recommendations(&json),
            Err(RadarDataError::InvalidData)
        );
    }

    #[test]
    fn maps_known_models_and_sanitizes_unknown_labels() {
        assert_eq!(model_display_name("gpt-5.6-sol"), "Sol");
        assert_eq!(model_display_name(" custom\u{0007}model "), "custommodel");
    }
}
