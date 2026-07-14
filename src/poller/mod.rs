use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::core::diagnose;

mod api;
mod credentials;
mod format;

pub use self::api::{is_past_reset, poll, PollError};
pub use self::credentials::credential_watch_snapshot;
pub use self::format::{format_line, time_until_display_change};

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub(super) fn cli_refresh_codex_token() {
    let started = Instant::now();
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
    diagnose::log(format!(
        "Windows Codex token refresh finished in {}ms",
        started.elapsed().as_millis()
    ));
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
