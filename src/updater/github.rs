use std::time::Duration;

use serde::Deserialize;

pub(super) const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
pub(super) const GITHUB_API_VERSION: &str = "2022-11-28";
pub(super) const RELEASE_ASSET_NAME: &str = "codex-usage-taskbar-monitor.exe";

use super::ReleaseDescriptor;

#[derive(Deserialize)]
pub(super) struct GitHubRelease {
    pub(super) tag_name: String,
    pub(super) assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
pub(super) struct GitHubAsset {
    pub(super) name: String,
    pub(super) browser_download_url: String,
}

pub(super) fn fetch_latest_release() -> Result<Option<ReleaseDescriptor>, String> {
    let (owner, repo) = github_repo()?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let agent = build_agent()?;

    let response = match agent
        .get(&url)
        .set("Accept", GITHUB_API_ACCEPT)
        .set("User-Agent", user_agent())
        .set("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(403 | 429, response)) => {
            return fetch_latest_release_from_redirect(owner, repo).map_err(|fallback_error| {
                format!(
                    "Unable to check GitHub releases via API: status code {}. Fallback check failed: {fallback_error}",
                    response.status()
                )
            });
        }
        Err(error) => return Err(format!("Unable to check GitHub releases: {error}")),
    };

    let release: GitHubRelease = response
        .into_json()
        .map_err(|e| format!("Unable to parse GitHub release data: {e}"))?;

    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    if !is_version_newer(&latest_version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(RELEASE_ASSET_NAME))
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|asset| asset.name.to_ascii_lowercase().ends_with(".exe"))
        })
        .ok_or_else(|| {
            "No Windows executable asset was found in the latest release.".to_string()
        })?;

    Ok(Some(ReleaseDescriptor {
        latest_version,
        asset_url: asset.browser_download_url.clone(),
    }))
}

fn fetch_latest_release_from_redirect(
    owner: &str,
    repo: &str,
) -> Result<Option<ReleaseDescriptor>, String> {
    let url = format!("https://github.com/{owner}/{repo}/releases/latest");
    let agent = build_agent_without_redirects()?;
    let response = agent
        .head(&url)
        .set("User-Agent", user_agent())
        .call()
        .map_err(|e| format!("Unable to check GitHub latest release redirect: {e}"))?;

    if !(300..400).contains(&response.status()) {
        return Err(format!(
            "GitHub latest release redirect returned unexpected status code {}",
            response.status()
        ));
    }

    let location = response.header("Location").ok_or_else(|| {
        "GitHub latest release redirect did not include a Location header".to_string()
    })?;
    let tag = release_tag_from_latest_location(location).ok_or_else(|| {
        format!("GitHub latest release redirect did not point to a release tag: {location}")
    })?;
    let latest_version = tag.trim_start_matches('v').to_string();
    if !is_version_newer(&latest_version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    Ok(Some(ReleaseDescriptor {
        latest_version,
        asset_url: github_release_asset_url(owner, repo, tag),
    }))
}

pub(super) fn build_agent() -> Result<ureq::Agent, String> {
    build_agent_with_redirects(None)
}

fn build_agent_without_redirects() -> Result<ureq::Agent, String> {
    build_agent_with_redirects(Some(0))
}

fn build_agent_with_redirects(redirects: Option<u32>) -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new()
        .map_err(|e| format!("Unable to initialize TLS support for update checks: {e}"))?;
    let mut builder = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .tls_connector(std::sync::Arc::new(tls));
    if let Some(redirects) = redirects {
        builder = builder.redirects(redirects);
    }

    Ok(builder.build())
}

fn github_repo() -> Result<(&'static str, &'static str), String> {
    let repository = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/');
    let parts: Vec<&str> = repository.split('/').collect();
    if parts.len() < 2 {
        return Err("Package repository URL is not configured for GitHub releases.".to_string());
    }

    let owner = parts[parts.len() - 2];
    let repo = parts[parts.len() - 1];
    if owner.is_empty() || repo.is_empty() {
        return Err("Package repository URL is not configured for GitHub releases.".to_string());
    }

    Ok((owner, repo))
}

pub(super) fn user_agent() -> &'static str {
    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
}

fn release_tag_from_latest_location(location: &str) -> Option<&str> {
    let (_, rest) = location.split_once("/releases/tag/")?;
    let tag = rest
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');

    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

fn github_release_asset_url(owner: &str, repo: &str, tag: &str) -> String {
    format!("https://github.com/{owner}/{repo}/releases/download/{tag}/{RELEASE_ASSET_NAME}")
}

pub(super) fn is_version_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

fn parse_version(version: &str) -> (u32, u32, u32) {
    let core = version.split('-').next().unwrap_or(version);
    let mut parts = core.split('.').map(|part| part.parse::<u32>().unwrap_or(0));

    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_release_tag_from_github_latest_redirect() {
        assert_eq!(
            release_tag_from_latest_location(
                "https://github.com/LiarCoder/codex-usage-taskbar-monitor/releases/tag/v1.5.2"
            ),
            Some("v1.5.2")
        );
        assert_eq!(
            release_tag_from_latest_location(
                "/LiarCoder/codex-usage-taskbar-monitor/releases/tag/v1.5.2?expanded=true"
            ),
            Some("v1.5.2")
        );
        assert_eq!(
            release_tag_from_latest_location("https://github.com/LiarCoder/repo/releases"),
            None
        );
    }

    #[test]
    fn builds_github_release_asset_url_from_tag() {
        assert_eq!(
            github_release_asset_url("LiarCoder", "codex-usage-taskbar-monitor", "v1.5.2"),
            "https://github.com/LiarCoder/codex-usage-taskbar-monitor/releases/download/v1.5.2/codex-usage-taskbar-monitor.exe"
        );
    }
}
