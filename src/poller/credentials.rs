use std::path::PathBuf;

use serde::Deserialize;

use crate::core::diagnose;

#[derive(Deserialize)]
pub(super) struct CodexAuthFile {
    pub(super) tokens: Option<CodexTokenData>,
}

#[derive(Clone, Deserialize)]
pub(super) struct CodexTokenData {
    pub(super) access_token: String,
    pub(super) account_id: Option<String>,
}

pub fn credential_watch_snapshot() -> String {
    codex_credential_signature()
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

pub(super) fn codex_auth_path() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME").map(PathBuf::from) {
        return Some(home.join("auth.json"));
    }
    Some(dirs::home_dir()?.join(".codex").join("auth.json"))
}

pub(super) fn read_codex_credentials() -> Option<CodexTokenData> {
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
