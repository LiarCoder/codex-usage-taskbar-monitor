use serde::Deserialize;

use super::client::{base_request, call, HttpResponse};
use super::{
    rank_candidates, recommendation_from_parts, ComputedRecommendations, SourceUpdate,
    MIN_RECOMMENDED_IQ,
};

const RESULTS_URL: &str = "https://codexradar.com/data/codex-four-task-results.json";
const RESULTS_SCHEMA: &str = "codex-four-task-results/1";
const SCORE_PROTOCOL: &str = "codex-four-weighted-150-v2";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
struct Results {
    schema: String,
    protocol: String,
    score_max: u32,
    task_weights: TaskWeights,
    publication_status: String,
    all_graded: bool,
    expected: usize,
    graded: usize,
    configurations: usize,
    rows: Vec<Row>,
}

#[derive(Deserialize, PartialEq)]
struct TaskWeights {
    combined: f64,
    pompeii: f64,
    pelican: f64,
    candy: f64,
}

#[derive(Deserialize)]
struct Row {
    model: String,
    effort: String,
    combined: Option<TaskResult>,
    pompeii: Option<TaskResult>,
    pelican: Option<TaskResult>,
    candy: Option<TaskResult>,
}

#[derive(Deserialize)]
struct TaskResult {
    score: Option<f64>,
    reward: Option<f64>,
    correct: Option<bool>,
    cost_kind: Option<String>,
    cost_usd: Option<f64>,
    model_elapsed_seconds: Option<f64>,
}

pub(super) fn fetch_recommendations(
    agent: &ureq::Agent,
    etag: Option<&str>,
) -> SourceUpdate<ComputedRecommendations> {
    let mut request = base_request(agent, RESULTS_URL);
    if let Some(etag) = etag {
        request = request.set("If-None-Match", etag);
    }
    match call(request, MAX_RESPONSE_BYTES, "ETag") {
        Ok(HttpResponse::NotModified) => SourceUpdate::NotModified,
        Ok(HttpResponse::Body { bytes, validator }) => match parse_recommendations(&bytes) {
            Ok(value) => SourceUpdate::Updated { value, validator },
            Err(error) => SourceUpdate::Failed(error),
        },
        Err(error) => SourceUpdate::Failed(format!("four-task request failed: {error}")),
    }
}

pub(super) fn parse_recommendations(bytes: &[u8]) -> Result<ComputedRecommendations, String> {
    let results: Results = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid four-task JSON: {error}"))?;
    let weights = TaskWeights {
        combined: 60.0,
        pompeii: 45.0,
        pelican: 22.5,
        candy: 22.5,
    };
    if results.schema != RESULTS_SCHEMA
        || results.protocol != SCORE_PROTOCOL
        || results.score_max != 150
        || results.task_weights != weights
        || results.publication_status != "complete"
        || !results.all_graded
        || results.rows.is_empty()
        || results.configurations != results.rows.len()
        || results.configurations.checked_mul(4) != Some(results.expected)
        || results.graded != results.expected
    {
        return Err("incompatible or incomplete four-task results".to_string());
    }

    let mut candidates = Vec::new();
    let mut complete_points = 0;
    for row in results.rows {
        let (Some(combined), Some(pompeii), Some(pelican), Some(candy)) =
            (&row.combined, &row.pompeii, &row.pelican, &row.candy)
        else {
            continue;
        };
        let (Some(combined_score), Some(pompeii_score), Some(pelican_score), Some(correct)) =
            (combined.score, pompeii.reward, pelican.score, candy.correct)
        else {
            continue;
        };
        if !valid_score(combined_score, 100.0)
            || !valid_score(pompeii_score, 1.0)
            || !valid_score(pelican_score, 100.0)
        {
            continue;
        }
        let score = combined_score / 100.0 * weights.combined
            + pompeii_score * weights.pompeii
            + pelican_score / 100.0 * weights.pelican
            + if correct { weights.candy } else { 0.0 };
        let mut cost = 0.0;
        let mut seconds = 0.0;
        let mut valid_metrics = true;
        for task in [combined, pompeii, pelican, candy] {
            let (Some(task_cost), Some(task_seconds)) = (task.cost_usd, task.model_elapsed_seconds)
            else {
                valid_metrics = false;
                break;
            };
            if task.cost_kind.as_deref() != Some("api_equivalent_estimate")
                || !task_cost.is_finite()
                || task_cost < 0.0
                || !task_seconds.is_finite()
                || task_seconds <= 0.0
            {
                valid_metrics = false;
                break;
            }
            cost += task_cost;
            seconds += task_seconds;
        }
        if !valid_metrics || !seconds.is_finite() || seconds <= 0.0 {
            continue;
        }
        let Some(mut candidate) = recommendation_from_parts(row.model, row.effort, score, cost, 4)
        else {
            continue;
        };
        complete_points += 1;
        if score < MIN_RECOMMENDED_IQ {
            continue;
        }
        // Keep the four-task total score separate from legacy pass-rate confidence bounds
        candidate.average_minutes = Some(seconds / 60.0);
        candidates.push(candidate);
    }
    if complete_points == 0 {
        return Err(
            "four-task results contain no complete scores with valid cost and duration".to_string(),
        );
    }

    let mut computed = rank_candidates(&candidates);
    computed.radar_bench = true;
    Ok(computed)
}

fn valid_score(score: f64, maximum: f64) -> bool {
    score.is_finite() && (0.0..=maximum).contains(&score)
}
