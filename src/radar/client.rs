use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use super::{parse_efficiency_recommendations, ComputedRecommendations};

const EFFICIENCY_URL: &str = "https://codexradar.com/api/intelligence-efficiency";
const EFFICIENCY_MAX_BYTES: usize = 32 * 1024 * 1024;

static HTTP_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FetchValidators {
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

pub(crate) fn fetch_recommendations(
    validators: &FetchValidators,
) -> SourceUpdate<ComputedRecommendations> {
    let agent = match build_agent() {
        Ok(agent) => agent,
        Err(message) => return SourceUpdate::Failed(message),
    };

    match super::bench::fetch_recommendations(agent) {
        Ok(value) => SourceUpdate::Updated {
            value,
            validator: None,
        },
        Err(bench_error) => {
            let mut update =
                fetch_computed_recommendations(agent, validators.efficiency_etag.as_deref());
            if let SourceUpdate::Failed(error) = &mut update {
                *error = format!("RadarBench unavailable: {bench_error}; {error}");
            }
            update
        }
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

pub(super) fn base_request(agent: &ureq::Agent, url: &str) -> ureq::Request {
    agent.get(url).set(
        "User-Agent",
        concat!("codex-usage-taskbar-monitor/", env!("CARGO_PKG_VERSION")),
    )
}

pub(super) enum HttpResponse {
    NotModified,
    Body {
        bytes: Vec<u8>,
        validator: Option<String>,
    },
}

pub(super) fn call(
    request: ureq::Request,
    maximum_bytes: usize,
    validator_header: &str,
) -> Result<HttpResponse, String> {
    let response = match request.call() {
        Ok(response) if response.status() == 304 => return Ok(HttpResponse::NotModified),
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
