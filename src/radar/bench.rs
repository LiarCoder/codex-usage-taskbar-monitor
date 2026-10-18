use serde::Deserialize;

use super::client::{base_request, call, HttpResponse};
use super::{
    rank_candidates, recommendation_from_parts, ComputedRecommendations, Recommendation,
    MIN_RECOMMENDED_IQ,
};

const BINDING_URL: &str = "https://codexradar.com/data/radar-bench-binding.json";
const SUMMARY_URL: &str = "https://codexradar.com/api/radar-bench-score";
const SCORE_VERSION: &str = "radar-bench-v2-150";
const SUMMARY_SCHEMA: &str = "radar-bench-public-summary-v6-150-details";
const MAX_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_SELECTIONS: usize = 64;
const CONCURRENCY: usize = 3;

#[derive(Deserialize)]
struct Binding {
    enabled: bool,
    interface_confirmed: bool,
    production_read_verified: bool,
    score_version: String,
    score_max: u32,
    total_tasks: u32,
    benchmark: String,
    catalog_version: String,
    task_set_sha256: String,
    model_efforts: Vec<Selection>,
}

#[derive(Deserialize)]
struct Selection {
    model: String,
    effort: String,
}

#[derive(Deserialize)]
struct Summary {
    schema: String,
    score_version: String,
    score_max: u32,
    benchmark: String,
    catalog_version: String,
    task_set_sha256: String,
    model: String,
    effort: String,
    score: Option<f64>,
    coverage: u32,
    required_tasks: u32,
    score_status: String,
    statistics_state: String,
    statistics: Option<Statistics>,
}

#[derive(Deserialize)]
struct Statistics {
    sample_count: u32,
    cost_basis: String,
    metrics: Metrics,
}

#[derive(Deserialize)]
struct Metrics {
    api_equivalent_usd: Metric,
    elapsed_ms: Metric,
}

#[derive(Deserialize)]
struct Metric {
    known_total: Option<f64>,
    known_samples: u32,
    missing_samples: u32,
}

impl Metric {
    fn mean(&self, sample_count: u32) -> Option<f64> {
        if self.known_samples == 0
            || self.known_samples.checked_add(self.missing_samples) != Some(sample_count)
        {
            return None;
        }
        let mean = self.known_total? / f64::from(self.known_samples);
        (mean.is_finite() && mean > 0.0).then_some(mean)
    }
}

pub(super) fn fetch_recommendations(
    agent: &ureq::Agent,
) -> Result<ComputedRecommendations, String> {
    let binding: Binding = read_json(base_request(agent, BINDING_URL))?;
    if !binding.enabled
        || !binding.interface_confirmed
        || !binding.production_read_verified
        || binding.score_version != SCORE_VERSION
        || binding.score_max != 150
        || binding.total_tasks != 64
        || binding.model_efforts.is_empty()
        || binding.model_efforts.len() > MAX_SELECTIONS
    {
        return Err("unsupported RadarBench binding".to_string());
    }

    let mut candidates = Vec::new();
    let mut first_error = None;
    // Match the site's bounded read concurrency without requesting task records
    for selections in binding.model_efforts.chunks(CONCURRENCY) {
        std::thread::scope(|scope| {
            let requests = selections
                .iter()
                .map(|selection| scope.spawn(|| fetch_candidate(agent, &binding, selection)))
                .collect::<Vec<_>>();
            for request in requests {
                match request.join() {
                    Ok(Ok(candidate)) => {
                        candidates.extend(candidate);
                    }
                    Ok(Err(error)) => {
                        first_error.get_or_insert(error);
                    }
                    Err(_) => {
                        first_error.get_or_insert_with(|| "RadarBench worker failed".to_string());
                    }
                }
            }
        });
    }
    if let Some(error) = first_error {
        return Err(error);
    }

    let mut computed = rank_candidates(&candidates);
    computed.radar_bench = true;
    Ok(computed)
}

fn fetch_candidate(
    agent: &ureq::Agent,
    binding: &Binding,
    selection: &Selection,
) -> Result<Option<Recommendation>, String> {
    let request = base_request(agent, SUMMARY_URL)
        .set("X-Radar-Bench-Selection", "user-selected-v1")
        .query("model", &selection.model)
        .query("effort", &selection.effort)
        .query("view", "summary");
    let summary: Summary = read_json(request)
        .map_err(|error| format!("{} {}: {error}", selection.model, selection.effort))?;
    if summary.schema != SUMMARY_SCHEMA
        || summary.score_version != SCORE_VERSION
        || summary.score_max != binding.score_max
        || summary.benchmark != binding.benchmark
        || summary.catalog_version != binding.catalog_version
        || summary.task_set_sha256 != binding.task_set_sha256
        || summary.model != selection.model
        || summary.effort != selection.effort
        || summary.required_tasks != binding.total_tasks
        || summary.coverage > binding.total_tasks
        || summary.score_status
            != match summary.coverage {
                0 => "missing_current_result",
                coverage if coverage == binding.total_tasks => "complete",
                _ => "provisional",
            }
        || match summary.score {
            Some(score) => {
                summary.coverage == 0 || !score.is_finite() || !(0.0..=150.0).contains(&score)
            }
            None => summary.coverage != 0,
        }
    {
        return Err("incompatible RadarBench summary".to_string());
    }
    if summary.coverage != binding.total_tasks
        || summary.score.is_none_or(|score| score < MIN_RECOMMENDED_IQ)
        || summary.statistics_state != "available"
    {
        return Ok(None);
    }
    let Some(statistics) = summary.statistics else {
        return Ok(None);
    };
    if statistics.cost_basis
        != "official_standard_api_equivalent; no estimates or claimed cash charges"
    {
        return Ok(None);
    }
    let Some(cost) = statistics
        .metrics
        .api_equivalent_usd
        .mean(statistics.sample_count)
    else {
        return Ok(None);
    };
    let Some(minutes) = statistics
        .metrics
        .elapsed_ms
        .mean(statistics.sample_count)
        .map(|ms| ms / 60_000.0)
    else {
        return Ok(None);
    };
    let Some(mut candidate) = recommendation_from_parts(
        summary.model,
        summary.effort,
        summary.score.unwrap(),
        cost,
        summary.coverage,
    ) else {
        return Ok(None);
    };
    candidate.average_minutes = Some(minutes);
    Ok(Some(candidate))
}

fn read_json<T: serde::de::DeserializeOwned>(request: ureq::Request) -> Result<T, String> {
    match call(request, MAX_RESPONSE_BYTES, "ETag")? {
        HttpResponse::Body { bytes, .. } => serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid RadarBench JSON: {error}")),
        HttpResponse::NotModified => {
            Err("RadarBench returned 304 without a cached summary".to_string())
        }
    }
}
