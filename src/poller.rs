use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use std::os::windows::process::CommandExt;

use crate::diagnose;
use crate::localization::Strings;
use crate::models::{AppUsageData, UsageData, UsageDisplayMode, UsageSection};

const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollError {
    AuthRequired,
    NoCredentials,
    TokenExpired,
    RequestFailed,
}

pub type CredentialWatchSnapshot = Vec<String>;

#[derive(Deserialize)]
struct CodexAuthFile {
    tokens: Option<CodexTokenData>,
}

#[derive(Clone, Deserialize)]
struct CodexTokenData {
    access_token: String,
    account_id: Option<String>,
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
    let usage = poll_codex()?;
    diagnose::log(format!(
        "Codex usage poll succeeded: session={:.1}% weekly={:.1}%",
        usage.session.percentage, usage.weekly.percentage
    ));
    Ok(usage)
}

fn poll_codex() -> Result<UsageData, PollError> {
    let creds = read_codex_credentials().ok_or_else(|| {
        diagnose::log("Codex usage poll failed: no Codex credentials found");
        PollError::NoCredentials
    })?;

    match fetch_codex_usage(&creds.access_token, creds.account_id.as_deref()) {
        Ok(data) => Ok(data),
        Err(PollError::AuthRequired) => {
            cli_refresh_codex_token();
            let refreshed = read_codex_credentials().ok_or(PollError::TokenExpired)?;
            fetch_codex_usage(&refreshed.access_token, refreshed.account_id.as_deref())
        }
        Err(error) => Err(error),
    }
}

fn cli_refresh_codex_token() {
    let codex_path = resolve_windows_codex_path();
    let is_cmd = codex_path.to_ascii_lowercase().ends_with(".cmd");
    let is_ps1 = codex_path.to_ascii_lowercase().ends_with(".ps1");
    diagnose::log(format!(
        "attempting Windows Codex token refresh via {codex_path}"
    ));
    let args = ["exec", "."];

    let mut command = if is_cmd {
        let mut command = Command::new("cmd.exe");
        command.arg("/c").arg(&codex_path).args(args);
        command
    } else if is_ps1 {
        let mut command = Command::new("powershell.exe");
        command
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&codex_path)
            .args(args);
        command
    } else {
        let mut command = Command::new(&codex_path);
        command.args(args);
        command
    };

    command
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            diagnose::log_error("unable to spawn Windows Codex token refresh", error);
            return;
        }
    };
    wait_for_refresh(&mut child);
}

fn wait_for_refresh(child: &mut std::process::Child) {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(30) => {
                let _ = child.kill();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

fn resolve_windows_codex_path() -> String {
    for name in ["codex.cmd", "codex.ps1", "codex.exe", "codex"] {
        if Command::new(name)
            .arg("--version")
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return name.to_string();
        }
    }
    "codex.cmd".to_string()
}

fn build_agent() -> Result<ureq::Agent, PollError> {
    let tls = native_tls::TlsConnector::new().map_err(|_| PollError::RequestFailed)?;
    Ok(ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .tls_connector(std::sync::Arc::new(tls))
        .build())
}

pub fn credential_watch_snapshot() -> CredentialWatchSnapshot {
    vec![codex_credential_signature()]
}

fn codex_credential_signature() -> String {
    let Some(path) = codex_auth_path() else {
        return "codex-auth|unavailable".to_string();
    };
    match std::fs::metadata(&path) {
        Ok(metadata) => format!(
            "{}|present|{}|{:?}",
            path.display(),
            metadata.len(),
            metadata.modified().ok()
        ),
        Err(_) => format!("{}|missing", path.display()),
    }
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

    let response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::Status(code, _)) if code == 401 || code == 403 => {
            diagnose::log(format!("Codex usage endpoint returned auth status {code}"));
            return Err(PollError::AuthRequired);
        }
        Err(error) => {
            diagnose::log_error("Codex usage endpoint request failed", error);
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
    if let Some(window) = details.primary_window.flatten() {
        data.session = codex_section_from_window(&window);
    }
    if let Some(window) = details.secondary_window.flatten() {
        data.weekly = codex_section_from_window(&window);
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

fn codex_auth_path() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME").map(PathBuf::from) {
        return Some(home.join("auth.json"));
    }
    Some(dirs::home_dir()?.join(".codex").join("auth.json"))
}

fn read_codex_credentials() -> Option<CodexTokenData> {
    let path = codex_auth_path()?;
    let content = std::fs::read_to_string(&path)
        .map_err(|error| {
            diagnose::log_error(
                &format!("unable to read Codex credentials at {}", path.display()),
                error,
            )
        })
        .ok()?;
    let auth: CodexAuthFile = serde_json::from_str(&content).ok()?;
    auth.tokens.filter(|tokens| !tokens.access_token.is_empty())
}

pub fn format_line(
    section: &UsageSection,
    strings: Strings,
    usage_display: UsageDisplayMode,
) -> String {
    let percentage = format!(
        "{:.0}%",
        usage_display.display_percentage(section.percentage)
    );
    let countdown = format_countdown(section.resets_at, strings);
    if countdown.is_empty() {
        percentage
    } else {
        format!("{percentage} · {countdown}")
    }
}

fn format_countdown(resets_at: Option<SystemTime>, strings: Strings) -> String {
    let Some(reset) = resets_at else {
        return String::new();
    };
    let remaining = match reset.duration_since(SystemTime::now()) {
        Ok(remaining) => remaining,
        Err(_) => return strings.now.to_string(),
    };
    format_countdown_from_secs(remaining.as_secs(), strings)
}

fn format_countdown_from_secs(total: u64, strings: Strings) -> String {
    if total >= 86_400 {
        format!("{}{}", total / 86_400, strings.day_suffix)
    } else if total >= 3_600 {
        format!("{}{}", total / 3_600, strings.hour_suffix)
    } else if total >= 60 {
        format!("{}{}", total / 60, strings.minute_suffix)
    } else {
        format!("{total}{}", strings.second_suffix)
    }
}

pub fn time_until_display_change(resets_at: Option<SystemTime>) -> Option<Duration> {
    let total = resets_at?.duration_since(SystemTime::now()).ok()?.as_secs();
    let bucket = if total >= 86_400 {
        total / 86_400 * 86_400
    } else if total >= 3_600 {
        total / 3_600 * 3_600
    } else if total >= 60 {
        total / 60 * 60
    } else {
        total
    };
    Some(Duration::from_secs(total.saturating_sub(bucket) + 1))
}

pub fn is_past_reset(data: &UsageData) -> bool {
    let now = SystemTime::now();
    [data.session.resets_at, data.weekly.resets_at]
        .into_iter()
        .flatten()
        .any(|reset| now.duration_since(reset).is_ok())
}

pub fn app_is_past_reset(data: &AppUsageData) -> bool {
    is_past_reset(data)
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
        assert_eq!(data.session.percentage, 42.0);
        assert_eq!(data.weekly.percentage, 64.0);
    }
}
