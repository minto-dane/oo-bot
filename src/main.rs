#![forbid(unsafe_code)]

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};
use discord_oo_bot::{
    audit::{
        AuditEventInput, AuditEventType, AuditQueryFilter, AuditStore, AuditStoreConfig,
        ExportFormat,
    },
    config::{
        ensure_startup_config_exists, load_startup_config_from_path, startup_config_path,
        write_startup_config_to_path, StartupConfig, StartupConfigError,
    },
    control::{
        bind_runtime_control_listener, control_socket_path, request_runtime_status,
        request_runtime_stop, serve_runtime_control, RuntimeControlCommand, RuntimeControlStatus,
    },
    domain::detector::build_detector,
    infra::discord_handler::{Handler, HandlerRuntimeMeta},
    operator_tui::{run_operator_tui, OperatorTuiEntry, OperatorTuiParams},
    sandbox::host::{SandboxConfig, WasmtimeSandboxAnalyzer},
    security::{
        core_governor::TrustedCore, hardening::detect_hardening_status, lsm::detect_lsm_status,
    },
};
use serenity::all::{Client, GatewayIntents};
use thiserror::Error;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

const TUI_AUDIT_LIMIT: usize = 200;

#[derive(Debug, Error)]
enum StartupError {
    #[error("missing required environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("invalid environment variable: {0}")]
    InvalidEnv(&'static str),
    #[error("invalid startup config: {0}")]
    InvalidStartupConfig(StartupConfigError),
    #[error("detector init failed: {0}")]
    DetectorInit(String),
    #[error("sandbox init failed: {0}")]
    SandboxInit(String),
    #[error("failed to create discord client: {0}")]
    ClientBuild(String),
    #[error("audit command failed: {0}")]
    AuditCommand(String),
    #[error("config command failed: {0}")]
    ConfigCommand(String),
    #[error("control command failed: {0}")]
    ControlCommand(String),
}

#[derive(Debug, Parser)]
#[command(name = "oo-bot")]
#[command(about = "oo-bot runtime and audit CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run,
    Tui {
        #[arg(value_enum, long, default_value_t = TuiEntryArg::Dashboard)]
        page: TuiEntryArg,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    Control {
        #[command(subcommand)]
        command: ControlCommands,
    },
    Audit {
        #[command(subcommand)]
        command: AuditCommands,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    Init {
        #[arg(long)]
        force: bool,
    },
    Setup,
    Edit,
}

#[derive(Debug, Subcommand)]
enum ControlCommands {
    Status,
    Stop,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TuiEntryArg {
    Dashboard,
    Setup,
    Diagnostics,
    Audit,
}

impl From<TuiEntryArg> for OperatorTuiEntry {
    fn from(value: TuiEntryArg) -> Self {
        match value {
            TuiEntryArg::Dashboard => OperatorTuiEntry::Dashboard,
            TuiEntryArg::Setup => OperatorTuiEntry::Setup,
            TuiEntryArg::Diagnostics => OperatorTuiEntry::Diagnostics,
            TuiEntryArg::Audit => OperatorTuiEntry::Audit,
        }
    }
}

#[derive(Debug, Subcommand)]
enum AuditCommands {
    Tail {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        start_ts_utc: Option<String>,
        #[arg(long)]
        end_ts_utc: Option<String>,
        #[arg(long)]
        event_type: Option<String>,
        #[arg(long)]
        detector_backend: Option<String>,
        #[arg(long)]
        suppressed_reason: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    Stats {
        #[arg(long)]
        start_ts_utc: Option<String>,
        #[arg(long)]
        end_ts_utc: Option<String>,
        #[arg(long)]
        event_type: Option<String>,
        #[arg(long)]
        detector_backend: Option<String>,
        #[arg(long)]
        suppressed_reason: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    Inspect {
        event_id: i64,
    },
    Verify {
        #[arg(long)]
        start_event_id: Option<i64>,
        #[arg(long)]
        end_event_id: Option<i64>,
    },
    Export {
        #[arg(long)]
        format: AuditExportFormatArg,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 10_000)]
        limit: usize,
        #[arg(long)]
        start_ts_utc: Option<String>,
        #[arg(long)]
        end_ts_utc: Option<String>,
        #[arg(long)]
        event_type: Option<String>,
        #[arg(long)]
        detector_backend: Option<String>,
        #[arg(long)]
        suppressed_reason: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    Tui {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AuditExportFormatArg {
    Jsonl,
    Csv,
    Parquet,
}

impl From<AuditExportFormatArg> for ExportFormat {
    fn from(value: AuditExportFormatArg) -> Self {
        match value {
            AuditExportFormatArg::Jsonl => ExportFormat::Jsonl,
            AuditExportFormatArg::Csv => ExportFormat::Csv,
            AuditExportFormatArg::Parquet => ExportFormat::Parquet,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    init_tracing();
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => run_bot().await,
        Commands::Tui { page } => run_tui_command(page),
        Commands::Config { command } => run_config_command(command),
        Commands::Control { command } => run_control_command(command).await,
        Commands::Audit { command } => run_audit_command(command),
    }
}

async fn run_bot() -> Result<(), StartupError> {
    let startup = prepare_startup_config(true)?;
    let token =
        std::env::var("DISCORD_TOKEN").map_err(|_| StartupError::MissingEnv("DISCORD_TOKEN"))?;
    validate_discord_token(&token)?;

    let detector = build_detector(startup.app.detector.backend, startup.app.detector.as_policy())
        .map_err(StartupError::DetectorInit)?;
    let bot_config = startup.app.bot.as_bot_config(&startup.app.detector);
    let runtime_cfg = startup.app.runtime.clone();

    let sandbox_cfg = load_sandbox_config()?;
    let analyzer = WasmtimeSandboxAnalyzer::new(sandbox_cfg).map_err(StartupError::SandboxInit)?;

    let mut core =
        TrustedCore::new_with_detector(Box::new(analyzer), detector, bot_config, runtime_cfg);

    let (budget_total, budget_remaining, budget_reset_after) = load_session_budget()?;
    core.update_session_budget(budget_total, budget_remaining, budget_reset_after);

    let lsm_status = detect_lsm_status();
    let hardening_status = detect_hardening_status();

    for warning in &lsm_status.warnings {
        warn!(warning = warning, "lsm detection warning");
    }
    for warning in &hardening_status.warnings {
        warn!(warning = warning, "hardening status warning");
    }

    let active_lsm = lsm_status.active_lsm_summary();
    let hardening_summary = hardening_status.summary();

    info!(
        config_fingerprint = %startup.config_fingerprint,
        detector_backend = ?startup.app.detector.backend,
        active_lsm = %active_lsm,
        hardening_status = %hardening_summary,
        "startup profile"
    );

    let audit_cfg = AuditStoreConfig {
        sqlite_path: startup.app.audit.sqlite_path.clone(),
        busy_timeout_ms: startup.app.audit.busy_timeout_ms,
        export_max_rows: startup.app.audit.export_max_rows,
        query_max_rows: startup.app.audit.query_max_rows,
    };

    let audit_store = match AuditStore::open_rw(&audit_cfg, startup.pseudo_id_hmac_key.clone()) {
        Ok(mut store) => {
            let mut redacted_config = startup.app.clone();
            if let Some(signature) = redacted_config.integrity.config_signature.as_mut() {
                signature.hmac_key_env = "<redacted>".to_string();
            }
            if let Some(key_env) = redacted_config.integrity.pseudo_id_hmac_key_env.as_mut() {
                *key_env = "<redacted>".to_string();
            }

            let config_json = serde_json::to_string(&redacted_config)
                .map_err(|err| StartupError::AuditCommand(err.to_string()))?;
            if let Err(err) =
                store.record_config_snapshot(&startup.config_fingerprint, &config_json)
            {
                warn!(error = %err, "failed to record config snapshot");
            }

            let event = AuditEventInput {
                event_type: AuditEventType::ProcessStart,
                binary_version: env!("CARGO_PKG_VERSION").to_string(),
                config_fingerprint: startup.config_fingerprint.clone(),
                detector_backend: format!("{:?}", startup.app.detector.backend),
                active_lsm: active_lsm.clone(),
                hardening_status: hardening_summary.clone(),
                ..AuditEventInput::default()
            };
            if let Err(err) = store.record_event(&event) {
                warn!(error = %err, "failed to record process_start audit event");
            }

            Some(Arc::new(Mutex::new(store)))
        }
        Err(err) => {
            warn!(error = %err, "audit store unavailable; continuing without sqlite audit write path");
            None
        }
    };

    if core.session_budget_low() {
        info!(remaining = budget_remaining, "session budget low: starting in degraded posture");
    }

    let shared_core = Arc::new(Mutex::new(core));

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(Handler {
            core: shared_core,
            audit: audit_store.clone(),
            runtime_meta: HandlerRuntimeMeta {
                binary_version: env!("CARGO_PKG_VERSION").to_string(),
                config_fingerprint: startup.config_fingerprint.clone(),
                active_lsm: active_lsm.clone(),
                hardening_status: hardening_summary.clone(),
            },
        })
        .await
        .map_err(|err| StartupError::ClientBuild(err.to_string()))?;

    let control_socket = control_socket_path(&startup.config_path);
    let control_listener =
        bind_runtime_control_listener(&control_socket).map_err(StartupError::ControlCommand)?;
    let control_status = RuntimeControlStatus {
        state: "running".to_string(),
        pid: std::process::id(),
        started_at_unix: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        config_path: startup.config_path.display().to_string(),
        config_fingerprint: startup.config_fingerprint.clone(),
        detector_backend: format!("{:?}", startup.app.detector.backend),
        active_lsm: active_lsm.clone(),
        hardening_status: hardening_summary.clone(),
        socket_path: control_socket.display().to_string(),
    };
    let (control_command_tx, mut control_command_rx) = tokio::sync::mpsc::channel(8);
    let (control_shutdown_tx, control_shutdown_rx) = tokio::sync::watch::channel(false);
    let control_server = tokio::spawn(serve_runtime_control(
        control_listener,
        control_socket.clone(),
        control_status.clone(),
        control_shutdown_rx,
        control_command_tx.clone(),
    ));
    let signal_supervisor = tokio::spawn(run_signal_supervisor(control_command_tx.clone()));
    let shard_manager = client.shard_manager.clone();
    let shutdown_supervisor = tokio::spawn(async move {
        if let Some(RuntimeControlCommand::Stop { source }) = control_command_rx.recv().await {
            info!(source = %source, "runtime stop requested");
            shard_manager.shutdown_all().await;
        }
    });

    info!(control_socket = %control_socket.display(), "starting bot");
    if let Err(err) = client.start().await {
        error!(error = %err, "discord client stopped with error");
    }

    let _ = control_shutdown_tx.send(true);
    signal_supervisor.abort();
    let _ = signal_supervisor.await;
    drop(control_command_tx);
    let _ = shutdown_supervisor.await;
    match control_server.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => warn!(error = %err, "runtime control server exited with error"),
        Err(err) => warn!(error = %err, "runtime control server join error"),
    }

    if let Some(store) = audit_store {
        let mut guard = match store.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let event = AuditEventInput {
            event_type: AuditEventType::ProcessShutdown,
            binary_version: env!("CARGO_PKG_VERSION").to_string(),
            config_fingerprint: startup.config_fingerprint,
            ..AuditEventInput::default()
        };
        if let Err(err) = guard.record_event(&event) {
            warn!(error = %err, "failed to record process_shutdown audit event");
        }
    }

    Ok(())
}

async fn run_control_command(command: ControlCommands) -> Result<(), StartupError> {
    let path = startup_config_path();

    match command {
        ControlCommands::Status => {
            let status = request_runtime_status(&path).map_err(StartupError::ControlCommand)?;
            println!("state={}", status.state);
            println!("pid={}", status.pid);
            println!("started_at_unix={}", status.started_at_unix);
            println!("config_path={}", status.config_path);
            println!("config_fingerprint={}", status.config_fingerprint);
            println!("detector_backend={}", status.detector_backend);
            println!("active_lsm={}", status.active_lsm);
            println!("hardening_status={}", status.hardening_status);
            println!("socket_path={}", status.socket_path);
        }
        ControlCommands::Stop => {
            let message =
                request_runtime_stop(&path, "cli").map_err(StartupError::ControlCommand)?;
            println!("{message}");
        }
    }

    Ok(())
}

fn run_audit_command(command: AuditCommands) -> Result<(), StartupError> {
    let startup = prepare_startup_config(false)?;

    if let AuditCommands::Tui { limit } = command {
        let _ = run_operator_tui(
            OperatorTuiEntry::Audit,
            OperatorTuiParams { startup, startup_created: false, audit_limit: limit },
        )
        .map_err(StartupError::AuditCommand)?;
        return Ok(());
    }

    let cfg = AuditStoreConfig {
        sqlite_path: startup.app.audit.sqlite_path,
        busy_timeout_ms: startup.app.audit.busy_timeout_ms,
        export_max_rows: startup.app.audit.export_max_rows,
        query_max_rows: startup.app.audit.query_max_rows,
    };

    let store = AuditStore::open_ro(&cfg).map_err(StartupError::AuditCommand)?;

    match command {
        AuditCommands::Tail {
            limit,
            start_ts_utc,
            end_ts_utc,
            event_type,
            detector_backend,
            suppressed_reason,
            mode,
        } => {
            let filter = AuditQueryFilter {
                start_ts_utc,
                end_ts_utc,
                event_type,
                detector_backend,
                suppressed_reason,
                mode,
                limit: Some(limit),
            };
            let rows = store.tail(&filter).map_err(StartupError::AuditCommand)?;
            for row in rows {
                println!(
                    "{} {} backend={} action={} reason={} mode={} hash={}",
                    row.event_id,
                    row.event_type,
                    row.detector_backend,
                    row.selected_action,
                    if row.suppressed_reason.is_empty() {
                        "-"
                    } else {
                        row.suppressed_reason.as_str()
                    },
                    row.mode,
                    row.row_hash
                );
            }
        }
        AuditCommands::Stats {
            start_ts_utc,
            end_ts_utc,
            event_type,
            detector_backend,
            suppressed_reason,
            mode,
        } => {
            let filter = AuditQueryFilter {
                start_ts_utc,
                end_ts_utc,
                event_type,
                detector_backend,
                suppressed_reason,
                mode,
                limit: None,
            };
            let stats = store.stats(&filter).map_err(StartupError::AuditCommand)?;
            println!("total={}", stats.total);
            println!(
                "by_event_type={}",
                serde_json::to_string(&stats.by_event_type).unwrap_or_default()
            );
            println!("by_backend={}", serde_json::to_string(&stats.by_backend).unwrap_or_default());
            println!(
                "by_suppressed_reason={}",
                serde_json::to_string(&stats.by_suppressed_reason).unwrap_or_default()
            );
            println!("by_mode={}", serde_json::to_string(&stats.by_mode).unwrap_or_default());
        }
        AuditCommands::Inspect { event_id } => {
            let row = store.inspect(event_id).map_err(StartupError::AuditCommand)?;
            match row {
                Some(row) => {
                    println!("{}", serde_json::to_string_pretty(&row).unwrap_or_default());
                }
                None => {
                    println!("event not found: {event_id}");
                }
            }
        }
        AuditCommands::Verify { start_event_id, end_event_id } => {
            let report =
                store.verify(start_event_id, end_event_id).map_err(StartupError::AuditCommand)?;
            println!("checked_rows={}", report.checked_rows);
            println!("broken_rows={}", report.broken_rows);
            for detail in report.details {
                println!("detail={detail}");
            }
        }
        AuditCommands::Export {
            format,
            out,
            limit,
            start_ts_utc,
            end_ts_utc,
            event_type,
            detector_backend,
            suppressed_reason,
            mode,
        } => {
            let filter = AuditQueryFilter {
                start_ts_utc,
                end_ts_utc,
                event_type,
                detector_backend,
                suppressed_reason,
                mode,
                limit: Some(limit),
            };
            let count =
                store.export(format.into(), &out, &filter).map_err(StartupError::AuditCommand)?;
            println!("exported_rows={count}");
            println!("output={}", out.display());
        }
        AuditCommands::Tui { .. } => unreachable!("tui handled before store open"),
    }

    Ok(())
}

fn run_tui_command(page: TuiEntryArg) -> Result<(), StartupError> {
    let (startup, created) = prepare_startup_config_with_creation(false)?;
    let _ = run_operator_tui(
        page.into(),
        OperatorTuiParams { startup, startup_created: created, audit_limit: TUI_AUDIT_LIMIT },
    )
    .map_err(StartupError::ConfigCommand)?;
    Ok(())
}

fn run_config_command(command: ConfigCommands) -> Result<(), StartupError> {
    let path = startup_config_path();

    match command {
        ConfigCommands::Init { force } => {
            if path.exists() && !force {
                println!("config already exists: {}", path.display());
                println!("use --force to overwrite it");
                return Ok(());
            }

            let config = StartupConfig::default();
            write_startup_config_to_path(&path, &config)
                .map_err(StartupError::InvalidStartupConfig)?;
            println!("wrote config: {}", path.display());
        }
        ConfigCommands::Setup | ConfigCommands::Edit => {
            let (startup, created) = prepare_startup_config_with_creation(false)?;
            let _ = run_operator_tui(
                OperatorTuiEntry::Setup,
                OperatorTuiParams {
                    startup: startup.clone(),
                    startup_created: created,
                    audit_limit: TUI_AUDIT_LIMIT,
                },
            )
            .map_err(StartupError::ConfigCommand)?;
            println!("setup closed: {}", startup.config_path.display());
        }
    }

    Ok(())
}

fn prepare_startup_config(
    launch_tui_on_first_run: bool,
) -> Result<discord_oo_bot::config::LoadedStartupConfig, StartupError> {
    prepare_startup_config_with_creation(launch_tui_on_first_run).map(|(startup, _)| startup)
}

fn prepare_startup_config_with_creation(
    launch_tui_on_first_run: bool,
) -> Result<(discord_oo_bot::config::LoadedStartupConfig, bool), StartupError> {
    let path = startup_config_path();
    let created =
        ensure_startup_config_exists(&path).map_err(StartupError::InvalidStartupConfig)?;
    let mut startup =
        load_startup_config_from_path(&path).map_err(StartupError::InvalidStartupConfig)?;

    if created && launch_tui_on_first_run && io::stdin().is_terminal() && io::stdout().is_terminal()
    {
        let _ = run_operator_tui(
            OperatorTuiEntry::Setup,
            OperatorTuiParams {
                startup: startup.clone(),
                startup_created: true,
                audit_limit: TUI_AUDIT_LIMIT,
            },
        )
        .map_err(StartupError::ConfigCommand)?;
        startup =
            load_startup_config_from_path(&path).map_err(StartupError::InvalidStartupConfig)?;
    }

    Ok((startup, created))
}

fn load_sandbox_config() -> Result<SandboxConfig, StartupError> {
    Ok(SandboxConfig {
        fuel_limit: read_env_u64("OO_SANDBOX_FUEL_LIMIT", 50_000)?,
        memory_limit_bytes: read_env_usize("OO_SANDBOX_MEMORY_BYTES", 65_536)?,
        table_elements_limit: read_env_usize("OO_SANDBOX_TABLE_ELEMENTS", 64)?,
        store_instance_limit: read_env_usize("OO_SANDBOX_INSTANCE_LIMIT", 4)?,
    })
}

fn load_session_budget() -> Result<(u32, u32, u64), StartupError> {
    let budget_total = read_env_u32("OO_SESSION_BUDGET_TOTAL", 1000)?;
    let budget_remaining = read_env_u32("OO_SESSION_BUDGET_REMAINING", budget_total)?;
    let budget_reset_after = read_env_u64("OO_SESSION_BUDGET_RESET_AFTER", 86_400)?;
    if budget_remaining > budget_total {
        return Err(StartupError::InvalidEnv("OO_SESSION_BUDGET_REMAINING"));
    }
    Ok((budget_total, budget_remaining, budget_reset_after))
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).compact().init();
}

#[cfg(unix)]
async fn run_signal_supervisor(command_tx: tokio::sync::mpsc::Sender<RuntimeControlCommand>) {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = match signal(SignalKind::interrupt()) {
        Ok(stream) => stream,
        Err(err) => {
            warn!(error = %err, "failed to install SIGINT supervisor");
            return;
        }
    };
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(stream) => stream,
        Err(err) => {
            warn!(error = %err, "failed to install SIGTERM supervisor");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = sigint.recv() => {
                warn!("SIGINT received; use `oo-bot control stop` or the operator TUI stop action to shut down");
            }
            _ = sigterm.recv() => {
                warn!("SIGTERM received; forwarding graceful stop request");
                let _ = command_tx
                    .send(RuntimeControlCommand::Stop {
                        source: "signal:SIGTERM".to_string(),
                    })
                    .await;
                break;
            }
        }
    }
}

#[cfg(not(unix))]
async fn run_signal_supervisor(_command_tx: tokio::sync::mpsc::Sender<RuntimeControlCommand>) {}

fn validate_discord_token(token: &str) -> Result<(), StartupError> {
    if token.trim().is_empty() || !token.contains('.') || token.len() < 50 {
        return Err(StartupError::InvalidEnv("DISCORD_TOKEN"));
    }
    Ok(())
}

fn read_env_u64(name: &'static str, default: u64) -> Result<u64, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<u64>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}

fn read_env_usize(name: &'static str, default: usize) -> Result<usize, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<usize>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}

fn read_env_u32(name: &'static str, default: u32) -> Result<u32, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<u32>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}
