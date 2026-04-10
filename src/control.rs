use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use tracing::warn;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::UnixStream as StdUnixStream;
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
#[cfg(unix)]
use tokio::sync::{mpsc, watch};
#[cfg(unix)]
use tokio::time::{timeout, Duration};

const CONTROL_SOCKET_NAME: &str = "control.sock";
const CONTROL_RUNTIME_DIR: &str = "/run/oo-bot";
const FALLBACK_SOCKET_PREFIX: &str = "oo-bot-control-";
const CONTROL_CONNECTION_TIMEOUT_SECS: u64 = 2;

#[derive(Debug, Clone)]
struct ControlSocketResolutionContext {
    explicit_socket_path: Option<String>,
    xdg_runtime_dir: Option<String>,
    runtime_dir_exists: bool,
}

fn current_control_socket_resolution_context() -> ControlSocketResolutionContext {
    ControlSocketResolutionContext {
        explicit_socket_path: std::env::var("OO_CONTROL_SOCKET_PATH").ok(),
        xdg_runtime_dir: std::env::var("XDG_RUNTIME_DIR").ok(),
        runtime_dir_exists: Path::new(CONTROL_RUNTIME_DIR).is_dir(),
    }
}

fn resolve_control_socket_path(
    config_path: &Path,
    context: &ControlSocketResolutionContext,
) -> PathBuf {
    if let Some(path) = &context.explicit_socket_path {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if context.runtime_dir_exists {
        return Path::new(CONTROL_RUNTIME_DIR).join(CONTROL_SOCKET_NAME);
    }

    if let Some(path) = &context.xdg_runtime_dir {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Path::new(trimmed).join("oo-bot").join(CONTROL_SOCKET_NAME);
        }
    }

    let digest = Sha256::digest(config_path.to_string_lossy().as_bytes());
    let suffix = hex::encode(digest);
    Path::new("/tmp").join(format!("{FALLBACK_SOCKET_PREFIX}{}.sock", &suffix[..16]))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeControlStatus {
    pub state: String,
    pub pid: u32,
    pub started_at_unix: u64,
    pub config_path: String,
    pub config_fingerprint: String,
    pub detector_backend: String,
    pub active_lsm: String,
    pub hardening_status: String,
    pub socket_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlRequest {
    Status,
    Stop { source: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlResponse {
    Status { status: RuntimeControlStatus },
    Ack { message: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeControlCommand {
    Stop { source: String },
}

#[must_use]
pub fn control_socket_path(config_path: &Path) -> PathBuf {
    let context = current_control_socket_resolution_context();
    resolve_control_socket_path(config_path, &context)
}

pub fn request_runtime_status(config_path: &Path) -> Result<RuntimeControlStatus, String> {
    match send_control_request(&control_socket_path(config_path), &ControlRequest::Status)? {
        ControlResponse::Status { status } => Ok(status),
        ControlResponse::Ack { message } | ControlResponse::Error { message } => Err(message),
    }
}

pub fn request_runtime_stop(config_path: &Path, source: &str) -> Result<String, String> {
    match send_control_request(
        &control_socket_path(config_path),
        &ControlRequest::Stop { source: source.to_string() },
    )? {
        ControlResponse::Ack { message } => Ok(message),
        ControlResponse::Status { .. } => {
            Err("unexpected status response to stop request".to_string())
        }
        ControlResponse::Error { message } => Err(message),
    }
}

#[cfg(unix)]
pub fn bind_runtime_control_listener(socket_path: &Path) -> Result<UnixListener, String> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    if socket_path.exists() {
        match StdUnixStream::connect(socket_path) {
            Ok(_) => {
                return Err(format!(
                    "runtime control socket already active at {}",
                    socket_path.display()
                ));
            }
            Err(_) => {
                fs::remove_file(socket_path).map_err(|err| err.to_string())?;
            }
        }
    }

    let listener = UnixListener::bind(socket_path).map_err(|err| err.to_string())?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|err| err.to_string())?;
    Ok(listener)
}

#[cfg(not(unix))]
pub fn bind_runtime_control_listener(_socket_path: &Path) -> Result<(), String> {
    Err("runtime control requires a unix platform".to_string())
}

#[cfg(unix)]
pub async fn serve_runtime_control(
    listener: UnixListener,
    socket_path: PathBuf,
    status: RuntimeControlStatus,
    mut shutdown_rx: watch::Receiver<bool>,
    command_tx: mpsc::Sender<RuntimeControlCommand>,
) -> Result<(), String> {
    let result = loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break Ok(());
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.map_err(|err| err.to_string())?;
                let status = status.clone();
                let command_tx = command_tx.clone();
                tokio::spawn(async move {
                    match timeout(
                        Duration::from_secs(CONTROL_CONNECTION_TIMEOUT_SECS),
                        handle_connection(stream, status, command_tx),
                    )
                    .await
                    {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => {
                            warn!(error = %err, "runtime control connection failed");
                        }
                        Err(_) => {
                            warn!("runtime control connection timed out");
                        }
                    }
                });
            }
        }
    };

    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    result
}

#[cfg(unix)]
async fn handle_connection(
    stream: UnixStream,
    status: RuntimeControlStatus,
    command_tx: mpsc::Sender<RuntimeControlCommand>,
) -> Result<(), String> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = AsyncBufReader::new(read_half);
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).await.map_err(|err| err.to_string())?;
    if bytes == 0 {
        return Ok(());
    }

    let request: ControlRequest =
        serde_json::from_str(line.trim_end()).map_err(|err| err.to_string())?;

    let response = match request {
        ControlRequest::Status => ControlResponse::Status { status },
        ControlRequest::Stop { source } => {
            command_tx
                .send(RuntimeControlCommand::Stop { source: source.clone() })
                .await
                .map_err(|err| err.to_string())?;
            ControlResponse::Ack { message: format!("stop request accepted via {source}") }
        }
    };

    let payload = serde_json::to_string(&response).map_err(|err| err.to_string())?;
    write_half.write_all(payload.as_bytes()).await.map_err(|err| err.to_string())?;
    write_half.write_all(b"\n").await.map_err(|err| err.to_string())?;
    write_half.shutdown().await.map_err(|err| err.to_string())
}

#[cfg(not(unix))]
pub async fn serve_runtime_control(
    _listener: (),
    _socket_path: PathBuf,
    _status: RuntimeControlStatus,
    _shutdown_rx: (),
    _command_tx: (),
) -> Result<(), String> {
    Err("runtime control requires a unix platform".to_string())
}

fn send_control_request(
    socket_path: &Path,
    request: &ControlRequest,
) -> Result<ControlResponse, String> {
    #[cfg(unix)]
    {
        let mut stream = StdUnixStream::connect(socket_path).map_err(|err| {
            format!("failed to connect runtime control socket at {}: {err}", socket_path.display())
        })?;
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));

        let payload = serde_json::to_string(request).map_err(|err| err.to_string())?;
        stream.write_all(payload.as_bytes()).map_err(|err| err.to_string())?;
        stream.write_all(b"\n").map_err(|err| err.to_string())?;
        stream.flush().map_err(|err| err.to_string())?;

        let mut line = String::new();
        let mut reader = BufReader::new(stream);
        reader.read_line(&mut line).map_err(|err| err.to_string())?;
        serde_json::from_str(line.trim_end()).map_err(|err| err.to_string())
    }

    #[cfg(not(unix))]
    {
        let _ = socket_path;
        let _ = request;
        Err("runtime control requires a unix platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runtime_control_status_and_stop_roundtrip() {
        let dir = tempdir().expect("temp dir");
        let socket_path = dir.path().join("control.sock");
        let listener = match bind_runtime_control_listener(&socket_path) {
            Ok(listener) => listener,
            Err(err) if err.contains("Operation not permitted") => return,
            Err(err) => panic!("bind control listener: {err}"),
        };
        let status = RuntimeControlStatus {
            state: "running".to_string(),
            pid: 1234,
            started_at_unix: 42,
            config_path: "config/oo-bot.yaml".to_string(),
            config_fingerprint: "fingerprint".to_string(),
            detector_backend: "morphological_reading".to_string(),
            active_lsm: "apparmor".to_string(),
            hardening_status: "ok".to_string(),
            socket_path: socket_path.display().to_string(),
        };
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (command_tx, mut command_rx) = mpsc::channel(2);
        let server = tokio::spawn(serve_runtime_control(
            listener,
            socket_path.clone(),
            status.clone(),
            shutdown_rx,
            command_tx,
        ));

        let status_response =
            send_control_request(&socket_path, &ControlRequest::Status).expect("status response");
        assert_eq!(status_response, ControlResponse::Status { status: status.clone() });

        let stop_response = send_control_request(
            &socket_path,
            &ControlRequest::Stop { source: "test".to_string() },
        )
        .expect("stop response");
        assert_eq!(
            stop_response,
            ControlResponse::Ack { message: "stop request accepted via test".to_string() }
        );
        assert_eq!(
            command_rx.recv().await,
            Some(RuntimeControlCommand::Stop { source: "test".to_string() })
        );

        shutdown_tx.send(true).expect("shutdown signal");
        let result = server.await.expect("server task join");
        assert!(result.is_ok());
        assert!(!socket_path.exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stalled_connection_does_not_block_following_requests() {
        let dir = tempdir().expect("temp dir");
        let socket_path = dir.path().join("control.sock");
        let listener = bind_runtime_control_listener(&socket_path).expect("bind control listener");
        let status = RuntimeControlStatus {
            state: "running".to_string(),
            pid: 5678,
            started_at_unix: 43,
            config_path: "config/oo-bot.yaml".to_string(),
            config_fingerprint: "fingerprint".to_string(),
            detector_backend: "morphological_reading".to_string(),
            active_lsm: "apparmor".to_string(),
            hardening_status: "ok".to_string(),
            socket_path: socket_path.display().to_string(),
        };
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (command_tx, _command_rx) = mpsc::channel(2);
        let server = tokio::spawn(serve_runtime_control(
            listener,
            socket_path.clone(),
            status.clone(),
            shutdown_rx,
            command_tx,
        ));

        let mut stalled =
            std::os::unix::net::UnixStream::connect(&socket_path).expect("stalled client connect");
        stalled.write_all(br#"{"kind":"status""#).expect("stalled client partial write");

        let status_response =
            send_control_request(&socket_path, &ControlRequest::Status).expect("status response");
        assert_eq!(status_response, ControlResponse::Status { status });

        drop(stalled);
        shutdown_tx.send(true).expect("shutdown signal");
        let result = server.await.expect("server task join");
        assert!(result.is_ok());
    }

    #[test]
    fn socket_path_prefers_explicit_env_override() {
        let dir = tempdir().expect("temp dir");
        let config_path = dir.path().join("config.yaml");
        let explicit = dir.path().join("explicit.sock");

        let context = ControlSocketResolutionContext {
            explicit_socket_path: Some(explicit.display().to_string()),
            xdg_runtime_dir: Some(dir.path().display().to_string()),
            runtime_dir_exists: true,
        };

        let resolved = resolve_control_socket_path(&config_path, &context);
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn socket_path_prefers_run_dir_over_xdg_when_no_explicit_override() {
        let dir = tempdir().expect("temp dir");
        let config_path = dir.path().join("config.yaml");

        let context = ControlSocketResolutionContext {
            explicit_socket_path: None,
            xdg_runtime_dir: Some(dir.path().display().to_string()),
            runtime_dir_exists: true,
        };

        let resolved = resolve_control_socket_path(&config_path, &context);
        let expected = Path::new(CONTROL_RUNTIME_DIR).join(CONTROL_SOCKET_NAME);
        assert_eq!(resolved, expected);
    }

    #[test]
    fn socket_path_ignores_blank_explicit_override() {
        let dir = tempdir().expect("temp dir");
        let config_path = dir.path().join("config.yaml");

        let context = ControlSocketResolutionContext {
            explicit_socket_path: Some("   ".to_string()),
            xdg_runtime_dir: Some(dir.path().display().to_string()),
            runtime_dir_exists: false,
        };

        let resolved = resolve_control_socket_path(&config_path, &context);
        let expected = dir.path().join("oo-bot").join(CONTROL_SOCKET_NAME);
        assert_eq!(resolved, expected);
    }

    #[test]
    fn socket_path_uses_xdg_runtime_dir_when_run_dir_missing() {
        let dir = tempdir().expect("temp dir");
        let config_path = dir.path().join("config.yaml");

        let context = ControlSocketResolutionContext {
            explicit_socket_path: None,
            xdg_runtime_dir: Some(dir.path().display().to_string()),
            runtime_dir_exists: false,
        };

        let resolved = resolve_control_socket_path(&config_path, &context);
        let expected = dir.path().join("oo-bot").join(CONTROL_SOCKET_NAME);
        assert_eq!(resolved, expected);
    }

    #[test]
    fn socket_path_falls_back_to_tmp_hash_when_no_runtime_dir_available() {
        let dir = tempdir().expect("temp dir");
        let config_path = dir.path().join("config.yaml");
        let other_config_path = dir.path().join("other-config.yaml");

        let context = ControlSocketResolutionContext {
            explicit_socket_path: None,
            xdg_runtime_dir: None,
            runtime_dir_exists: false,
        };

        let resolved = resolve_control_socket_path(&config_path, &context);
        let resolved_other = resolve_control_socket_path(&other_config_path, &context);
        let resolved_str = resolved.display().to_string();

        assert!(resolved.starts_with("/tmp"));
        assert!(resolved_str.contains(FALLBACK_SOCKET_PREFIX));
        assert!(resolved_str.ends_with(".sock"));
        assert_ne!(resolved, resolved_other);
    }
}
