use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::core::diagnose;
use crate::core::models::{UsageData, UsageSection};

use super::credentials::read_codex_credentials;

const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

static HTTP_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollError {
    AuthRequired,
    NoCredentials,
    TokenExpired,
    RequestFailed,
}

#[derive(Deserialize)]
struct CodexUsageResponse {
    rate_limit: Option<Option<Box<CodexRateLimitDetails>>>,
}

#[derive(Deserialize)]
struct CodexRateLimitDetails {
    primary_window: Option<Option<Box<CodexRateLimitWindow>>>,
    secondary_window: Option<Option<Box<CodexRateLimitWindow>>>,
}

#[derive(Deserialize)]
struct CodexRateLimitWindow {
    used_percent: f64,
    reset_at: i64,
}

pub fn poll() -> Result<UsageData, PollError> {
    let started = Instant::now();
    diagnose::log("Codex usage poll started");

    match poll_codex() {
        Ok(usage) => {
            diagnose::log(format!(
                "Codex usage poll succeeded in {}ms: session={:?} weekly={:?}",
                started.elapsed().as_millis(),
                usage.session.as_ref().map(|section| section.percentage),
                usage.weekly.as_ref().map(|section| section.percentage)
            ));
            Ok(usage)
        }
        Err(error) => {
            diagnose::log(format!(
                "Codex usage poll failed in {}ms: {error:?}",
                started.elapsed().as_millis()
            ));
            Err(error)
        }
    }
}

fn poll_codex() -> Result<UsageData, PollError> {
    let creds = read_codex_credentials().ok_or_else(|| {
        diagnose::log("Codex usage poll failed: no Codex credentials found");
        PollError::NoCredentials
    })?;

    match fetch_codex_usage(&creds.access_token, creds.account_id.as_deref()) {
        Ok(data) => Ok(data),
        Err(PollError::AuthRequired) => {
            super::cli_refresh_codex_token();
            let refreshed = read_codex_credentials().ok_or(PollError::TokenExpired)?;
            fetch_codex_usage(&refreshed.access_token, refreshed.account_id.as_deref())
        }
        Err(error) => Err(error),
    }
}

fn build_agent() -> Result<&'static ureq::Agent, PollError> {
    if let Some(agent) = HTTP_AGENT.get() {
        return Ok(agent);
    }

    let tls = native_tls::TlsConnector::new().map_err(|_| PollError::RequestFailed)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .tls_connector(std::sync::Arc::new(tls))
        .build();
    let _ = HTTP_AGENT.set(agent);

    Ok(HTTP_AGENT
        .get()
        .expect("HTTP agent is initialized before use"))
}

fn fetch_codex_usage(token: &str, account_id: Option<&str>) -> Result<UsageData, PollError> {
    let agent = build_agent()?;
    let mut request = agent
        .get(CODEX_USAGE_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "codex-cli");
    if let Some(account_id) = account_id.filter(|value| !value.is_empty()) {
        request = request.set("ChatGPT-Account-Id", account_id);
    }

    let request_started = Instant::now();
    let response = match request.call() {
        Ok(response) => {
            diagnose::log(format!(
                "Codex usage endpoint responded with {} in {}ms",
                response.status(),
                request_started.elapsed().as_millis()
            ));
            response
        }
        Err(ureq::Error::Status(code, _)) if code == 401 || code == 403 => {
            diagnose::log(format!(
                "Codex usage endpoint returned auth status {code} in {}ms",
                request_started.elapsed().as_millis()
            ));
            return Err(PollError::AuthRequired);
        }
        Err(error) => {
            diagnose::log_error(
                &format!(
                    "Codex usage endpoint request failed in {}ms",
                    request_started.elapsed().as_millis()
                ),
                error,
            );
            return Err(PollError::RequestFailed);
        }
    };

    let response: CodexUsageResponse = response.into_json().map_err(|error| {
        diagnose::log_error("unable to parse Codex usage response", error);
        PollError::RequestFailed
    })?;
    codex_usage_from_response(response).ok_or(PollError::RequestFailed)
}

fn codex_usage_from_response(response: CodexUsageResponse) -> Option<UsageData> {
    let details = *response.rate_limit.flatten()?;
    let mut data = UsageData::default();
    let primary = details.primary_window.flatten();
    let secondary = details.secondary_window.flatten();

    match (primary, secondary) {
        // Legacy responses expose both windows in their original order.
        (Some(primary), Some(secondary)) => {
            data.session = Some(codex_section_from_window(&primary));
            data.weekly = Some(codex_section_from_window(&secondary));
        }
        // Current responses can expose one long-window limit through the
        // primary field after the 5-hour window has been retired.
        (Some(primary), None) => {
            data.weekly = Some(codex_section_from_window(&primary));
        }
        (None, Some(secondary)) => {
            data.weekly = Some(codex_section_from_window(&secondary));
        }
        (None, None) => {}
    }
    Some(data)
}

fn codex_section_from_window(window: &CodexRateLimitWindow) -> UsageSection {
    UsageSection {
        percentage: window.used_percent,
        resets_at: u64::try_from(window.reset_at)
            .ok()
            .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds)),
    }
}

pub fn is_past_reset(data: &UsageData) -> bool {
    let now = SystemTime::now();
    [data.session.as_ref(), data.weekly.as_ref()]
        .into_iter()
        .flatten()
        .filter_map(|section| section.resets_at)
        .any(|reset| now.duration_since(reset).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_codex_rate_limit_windows() {
        let response: CodexUsageResponse = serde_json::from_str(
            r#"{"rate_limit":{"primary_window":{"used_percent":42,"reset_at":100},"secondary_window":{"used_percent":64,"reset_at":200}}}"#,
        ).unwrap();
        let data = codex_usage_from_response(response).unwrap();
        assert_eq!(data.session.unwrap().percentage, 42.0);
        assert_eq!(data.weekly.unwrap().percentage, 64.0);
    }

    #[test]
    fn maps_a_lone_primary_window_to_the_7_day_display() {
        let response: CodexUsageResponse = serde_json::from_str(
            r#"{"rate_limit":{"primary_window":{"used_percent":64,"reset_at":200}}}"#,
        )
        .unwrap();
        let data = codex_usage_from_response(response).unwrap();

        assert!(data.session.is_none());
        assert_eq!(data.weekly.unwrap().percentage, 64.0);
    }
}
