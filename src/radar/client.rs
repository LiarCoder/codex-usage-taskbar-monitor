use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use super::{
    parse_efficiency_recommendations, parse_radar_recommendation, ComputedRecommendations,
    RadarRecommendations,
};

const RADAR_INSIGHTS_URL: &str = "https://codexradar.com/api/radar-insights";
const EFFICIENCY_URL: &str = "https://codexradar.com/api/intelligence-efficiency";
const RADAR_INSIGHTS_MAX_BYTES: usize = 128 * 1024;
const EFFICIENCY_MAX_BYTES: usize = 8 * 1024 * 1024;

static HTTP_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FetchValidators {
    pub(crate) radar_last_modified: Option<String>,
    pub(crate) efficiency_etag: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SourceUpdate<T> {
    Updated { value: T, validator: Option<String> },
    NotModified,
    Failed(String),
}

impl<T> SourceUpdate<T> {
    pub(crate) fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RadarRefreshResult {
    pub(crate) radar: SourceUpdate<RadarRecommendations>,
    pub(crate) computed: SourceUpdate<ComputedRecommendations>,
}

pub(crate) fn fetch_recommendations(validators: &FetchValidators) -> RadarRefreshResult {
    let agent = match build_agent() {
        Ok(agent) => agent,
        Err(message) => {
            return RadarRefreshResult {
                radar: SourceUpdate::Failed(message.clone()),
                computed: SourceUpdate::Failed(message),
            }
        }
    };

    RadarRefreshResult {
        radar: fetch_radar_recommendation(agent, validators.radar_last_modified.as_deref()),
        computed: fetch_computed_recommendations(agent, validators.efficiency_etag.as_deref()),
    }
}

fn build_agent() -> Result<&'static ureq::Agent, String> {
    if let Some(agent) = HTTP_AGENT.get() {
        return Ok(agent);
    }

    let tls = native_tls::TlsConnector::new()
        .map_err(|error| format!("unable to initialize TLS: {error}"))?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .tls_connector(std::sync::Arc::new(tls))
        .build();
    let _ = HTTP_AGENT.set(agent);

    HTTP_AGENT
        .get()
        .ok_or_else(|| "unable to initialize HTTP agent".to_string())
}

fn fetch_radar_recommendation(
    agent: &ureq::Agent,
    last_modified: Option<&str>,
) -> SourceUpdate<RadarRecommendations> {
    let mut request = base_request(agent, RADAR_INSIGHTS_URL);
    if let Some(last_modified) = last_modified {
        request = request.set("If-Modified-Since", last_modified);
    }

    match call(request, RADAR_INSIGHTS_MAX_BYTES, "Last-Modified") {
        Ok(HttpResponse::NotModified) => SourceUpdate::NotModified,
        Ok(HttpResponse::Body { bytes, validator }) => match parse_radar_recommendation(&bytes) {
            Ok(value) => SourceUpdate::Updated { value, validator },
            Err(error) => SourceUpdate::Failed(format!("unable to parse radar insights: {error}")),
        },
        Err(message) => SourceUpdate::Failed(format!("radar insights request failed: {message}")),
    }
}

fn fetch_computed_recommendations(
    agent: &ureq::Agent,
    etag: Option<&str>,
) -> SourceUpdate<ComputedRecommendations> {
    let mut request = base_request(agent, EFFICIENCY_URL);
    if let Some(etag) = etag {
        request = request.set("If-None-Match", etag);
    }

    match call(request, EFFICIENCY_MAX_BYTES, "ETag") {
        Ok(HttpResponse::NotModified) => SourceUpdate::NotModified,
        Ok(HttpResponse::Body { bytes, validator }) => {
            match parse_efficiency_recommendations(&bytes) {
                Ok(value) => SourceUpdate::Updated { value, validator },
                Err(error) => {
                    SourceUpdate::Failed(format!("unable to parse efficiency data: {error}"))
                }
            }
        }
        Err(message) => SourceUpdate::Failed(format!("efficiency request failed: {message}")),
    }
}

fn base_request(agent: &ureq::Agent, url: &str) -> ureq::Request {
    agent.get(url).set(
        "User-Agent",
        concat!("codex-usage-taskbar-monitor/", env!("CARGO_PKG_VERSION")),
    )
}

enum HttpResponse {
    NotModified,
    Body {
        bytes: Vec<u8>,
        validator: Option<String>,
    },
}

fn call(
    request: ureq::Request,
    maximum_bytes: usize,
    validator_header: &str,
) -> Result<HttpResponse, String> {
    let response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::Status(304, _)) => return Ok(HttpResponse::NotModified),
        Err(ureq::Error::Status(code, _)) => return Err(format!("HTTP {code}")),
        Err(error) => return Err(error.to_string()),
    };

    if let Some(content_length) = response
        .header("Content-Length")
        .and_then(|value| value.parse::<usize>().ok())
    {
        if content_length > maximum_bytes {
            return Err(format!("response exceeds {maximum_bytes} bytes"));
        }
    }

    let validator = response.header(validator_header).map(str::to_string);
    let mut bytes = Vec::with_capacity(maximum_bytes.min(64 * 1024));
    response
        .into_reader()
        .take((maximum_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum_bytes {
        return Err(format!("response exceeds {maximum_bytes} bytes"));
    }

    Ok(HttpResponse::Body { bytes, validator })
}
