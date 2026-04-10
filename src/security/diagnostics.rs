use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::audit::{AuditQueryFilter, AuditStore, AuditStoreConfig};
use crate::config::{DependencySecurityCheckMode, LoadedStartupConfig};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfCheckItem {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSelfCheckReport {
    pub healthy: bool,
    pub items: Vec<SelfCheckItem>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityDiagnosticsMode {
    Offline,
    Online,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCheckResult {
    pub tool: String,
    pub status: CheckStatus,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub network_used: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityDiagnosticsReport {
    pub generated_at_utc: String,
    pub mode: SecurityDiagnosticsMode,
    pub high_or_critical_found: bool,
    pub results: Vec<ToolCheckResult>,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolExecutionPolicy {
    pub timeout: Duration,
    pub output_cap_bytes: usize,
    pub allow_network: bool,
}

impl Default for ToolExecutionPolicy {
    fn default() -> Self {
        Self { timeout: Duration::from_secs(30), output_cap_bytes: 32 * 1024, allow_network: false }
    }
}

pub fn run_local_self_check(startup: &LoadedStartupConfig) -> LocalSelfCheckReport {
    let mut items = Vec::new();

    items.push(check_file_exists("lockfile", Path::new("Cargo.lock")));
    items.push(check_file_exists(
        "pinned_hardened_toolchain",
        Path::new("ci/hardened-x64/rust-toolchain.toml"),
    ));
    items.push(check_file_exists("hardening_verifier", Path::new("scripts/verify_hardening.sh")));

    if startup.app.diagnostics.verify_generated_artifacts {
        items.push(check_file_exists("startup_config", &startup.config_path));
        items.push(check_file_exists(
            "hardening_script",
            Path::new("scripts/build_hardened_x64.sh"),
        ));
    }

    items.push(check_file_exists(
        "security_snapshot",
        &startup.app.diagnostics.security_snapshot_path,
    ));

    if startup.config_fingerprint.len() == 64 {
        items.push(SelfCheckItem {
            name: "config_fingerprint".to_string(),
            status: CheckStatus::Pass,
            detail: "sha256 fingerprint is present".to_string(),
        });
    } else {
        items.push(SelfCheckItem {
            name: "config_fingerprint".to_string(),
            status: CheckStatus::Fail,
            detail: "fingerprint is missing or malformed".to_string(),
        });
    }

    items.push(check_audit_health(
        &AuditStoreConfig {
            sqlite_path: startup.app.audit.sqlite_path.clone(),
            busy_timeout_ms: startup.app.audit.busy_timeout_ms,
            export_max_rows: startup.app.audit.export_max_rows,
            query_max_rows: startup.app.audit.query_max_rows,
        },
        startup.app.diagnostics.audit_verify_max_rows,
    ));

    let healthy = items.iter().all(|item| item.status != CheckStatus::Fail);
    LocalSelfCheckReport { healthy, items }
}

pub fn mode_from_dependency_policy(mode: DependencySecurityCheckMode) -> SecurityDiagnosticsMode {
    match mode {
        DependencySecurityCheckMode::Disabled | DependencySecurityCheckMode::OfflineSnapshot => {
            SecurityDiagnosticsMode::Offline
        }
    }
}

pub fn run_security_diagnostics(
    mode: SecurityDiagnosticsMode,
    policy: ToolExecutionPolicy,
) -> Result<SecurityDiagnosticsReport, String> {
    if mode == SecurityDiagnosticsMode::Online && !policy.allow_network {
        return Err("online diagnostics require explicit --allow-network opt-in".to_string());
    }

    let mut results = Vec::new();
    let network_used = mode == SecurityDiagnosticsMode::Online;

    let cargo_audit_args = if mode == SecurityDiagnosticsMode::Offline {
        vec!["audit", "--no-fetch"]
    } else {
        vec!["audit"]
    };
    results.push(run_tool("cargo-audit", "cargo", &cargo_audit_args, policy, network_used));
    results.push(run_tool("cargo-deny", "cargo", &["deny", "check"], policy, network_used));
    results.push(run_tool(
        "cargo-geiger",
        "cargo",
        &["geiger", "--all-features"],
        policy,
        network_used,
    ));

    let high_or_critical_found = results.iter().any(|result| {
        let lower = result.summary.to_ascii_lowercase();
        lower.contains("critical") || lower.contains("high")
    });

    Ok(SecurityDiagnosticsReport {
        generated_at_utc: now_utc_string(),
        mode,
        high_or_critical_found,
        results,
    })
}

pub fn write_security_snapshot(
    report: &SecurityDiagnosticsReport,
    path: &Path,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut encoded = serde_json::to_string_pretty(report).map_err(|err| err.to_string())?;
    encoded.push('\n');
    fs::write(path, encoded).map_err(|err| err.to_string())
}

fn run_tool(
    tool: &str,
    program: &str,
    args: &[&str],
    policy: ToolExecutionPolicy,
    network_used: bool,
) -> ToolCheckResult {
    let invocation = format!("{} {}", program, args.join(" "));
    let spawn =
        Command::new(program).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();

    let mut child = match spawn {
        Ok(child) => child,
        Err(err) => {
            return ToolCheckResult {
                tool: tool.to_string(),
                status: CheckStatus::Warn,
                exit_code: None,
                timed_out: false,
                network_used,
                summary: format!("tool missing or failed to spawn: {}", err),
            };
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_handle = thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut stream) = stdout {
            let _ = stream.read_to_end(&mut buffer);
        }
        buffer
    });
    let stderr_handle = thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut stream) = stderr {
            let _ = stream.read_to_end(&mut buffer);
        }
        buffer
    });

    let start = Instant::now();
    let mut timed_out = false;
    let exit_code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {
                if start.elapsed() > policy.timeout {
                    timed_out = true;
                    let _ = child.kill();
                    let status = child.wait().ok();
                    break status.and_then(|s| s.code());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break None,
        }
    };

    let mut combined = Vec::new();
    if let Ok(stdout_bytes) = stdout_handle.join() {
        combined.extend_from_slice(&stdout_bytes);
    }
    if let Ok(stderr_bytes) = stderr_handle.join() {
        combined.extend_from_slice(&stderr_bytes);
    }

    let summary = summarize_and_redact(&combined, policy.output_cap_bytes);
    let lower = summary.to_ascii_lowercase();
    let missing_subcommand = lower.contains("no such command") || lower.contains("not found");

    let status = if timed_out || missing_subcommand {
        CheckStatus::Warn
    } else if exit_code == Some(0) {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };

    ToolCheckResult {
        tool: tool.to_string(),
        status,
        exit_code,
        timed_out,
        network_used,
        summary: if summary.is_empty() {
            format!("invocation={} exit_code={:?}", invocation, exit_code)
        } else {
            summary
        },
    }
}

fn summarize_and_redact(bytes: &[u8], cap: usize) -> String {
    let cap = cap.max(64);
    let (slice, truncated) = if bytes.len() > cap { (&bytes[..cap], true) } else { (bytes, false) };
    let text = String::from_utf8_lossy(slice);
    let mut summary = redact_sensitive(text.trim());
    if truncated {
        summary.push_str(" [truncated]");
    }
    summary
}

pub fn redact_sensitive(input: &str) -> String {
    let mut output = input.to_string();
    for key in ["DISCORD_TOKEN", "OO_PSEUDO_ID_HMAC_KEY", "TOKEN", "SECRET", "PASSWORD"] {
        output = redact_key_value_pair(&output, key);
    }
    output
}

fn redact_key_value_pair(input: &str, key: &str) -> String {
    let mut output = String::new();
    for line in input.lines() {
        let upper = line.to_ascii_uppercase();
        if let Some(pos) = upper.find(&format!("{key}=")) {
            let end = pos + key.len() + 1;
            output.push_str(&line[..end]);
            output.push_str("<redacted>");
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output.trim_end().to_string()
}

fn check_file_exists(name: &str, path: &Path) -> SelfCheckItem {
    if path.exists() {
        SelfCheckItem {
            name: name.to_string(),
            status: CheckStatus::Pass,
            detail: format!("{} exists", path.display()),
        }
    } else {
        SelfCheckItem {
            name: name.to_string(),
            status: CheckStatus::Warn,
            detail: format!("{} is missing", path.display()),
        }
    }
}

fn check_audit_health(cfg: &AuditStoreConfig, max_rows: usize) -> SelfCheckItem {
    if !cfg.sqlite_path.exists() {
        return SelfCheckItem {
            name: "audit_db_health".to_string(),
            status: CheckStatus::Warn,
            detail: "audit sqlite does not exist yet".to_string(),
        };
    }

    let store = match AuditStore::open_ro(cfg) {
        Ok(store) => store,
        Err(err) => {
            return SelfCheckItem {
                name: "audit_db_health".to_string(),
                status: CheckStatus::Fail,
                detail: format!("failed to open audit sqlite: {err}"),
            };
        }
    };

    let last = match store.tail(&AuditQueryFilter { limit: Some(1), ..AuditQueryFilter::default() })
    {
        Ok(rows) => rows,
        Err(err) => {
            return SelfCheckItem {
                name: "audit_db_health".to_string(),
                status: CheckStatus::Fail,
                detail: format!("tail failed: {err}"),
            };
        }
    };

    if last.is_empty() {
        return SelfCheckItem {
            name: "audit_db_health".to_string(),
            status: CheckStatus::Pass,
            detail: "audit db is empty".to_string(),
        };
    }

    let end = last[0].event_id;
    let start = end.saturating_sub(max_rows as i64).saturating_add(1);
    match store.verify(Some(start), Some(end)) {
        Ok(report) if report.broken_rows == 0 => SelfCheckItem {
            name: "audit_db_health".to_string(),
            status: CheckStatus::Pass,
            detail: format!("verified {} recent rows", report.checked_rows),
        },
        Ok(report) => SelfCheckItem {
            name: "audit_db_health".to_string(),
            status: CheckStatus::Fail,
            detail: format!("broken_rows={} in recent verify", report.broken_rows),
        },
        Err(err) => SelfCheckItem {
            name: "audit_db_health".to_string(),
            status: CheckStatus::Fail,
            detail: format!("verify failed: {err}"),
        },
    }
}

fn now_utc_string() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::{
        redact_sensitive, run_security_diagnostics, write_security_snapshot, CheckStatus,
        SecurityDiagnosticsMode, ToolExecutionPolicy,
    };

    #[test]
    fn online_mode_requires_explicit_opt_in() {
        let report = run_security_diagnostics(
            SecurityDiagnosticsMode::Online,
            ToolExecutionPolicy { allow_network: false, ..ToolExecutionPolicy::default() },
        );
        assert!(report.is_err());
    }

    #[test]
    fn offline_mode_runs_without_network_opt_in() {
        let report = run_security_diagnostics(
            SecurityDiagnosticsMode::Offline,
            ToolExecutionPolicy {
                timeout: Duration::from_millis(200),
                output_cap_bytes: 1024,
                allow_network: false,
            },
        )
        .expect("offline diagnostics");
        assert_eq!(report.mode, SecurityDiagnosticsMode::Offline);
        assert!(report.results.iter().all(|result| !result.network_used));
    }

    #[test]
    fn redaction_masks_token_values() {
        let input = "DISCORD_TOKEN=abc123\nOO_PSEUDO_ID_HMAC_KEY=secret-value";
        let redacted = redact_sensitive(input);
        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("secret-value"));
        assert!(redacted.contains("<redacted>"));
    }

    #[test]
    fn diagnostics_snapshot_is_written() {
        let dir = tempdir().expect("tempdir");
        let path = PathBuf::from(dir.path()).join("security-report.json");

        let report = run_security_diagnostics(
            SecurityDiagnosticsMode::Offline,
            ToolExecutionPolicy {
                timeout: Duration::from_millis(200),
                output_cap_bytes: 512,
                allow_network: false,
            },
        )
        .expect("diagnostics");

        write_security_snapshot(&report, &path).expect("write snapshot");
        let encoded = std::fs::read_to_string(path).expect("read snapshot");
        assert!(encoded.contains("offline"));
    }

    #[test]
    fn tool_missing_is_graceful_warning() {
        let result = super::run_tool(
            "missing-tool",
            "definitely-missing-binary-for-oo-bot",
            &[],
            ToolExecutionPolicy {
                timeout: Duration::from_millis(100),
                output_cap_bytes: 256,
                allow_network: false,
            },
            false,
        );
        assert_eq!(result.status, CheckStatus::Warn);
    }
}
