use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::audit::{
    AuditQueryFilter, AuditStats, AuditStore, AuditStoreConfig, AuditEventRow, SCHEMA_VERSION,
};
use crate::config::{
    canonical_startup_config, validate_startup_config, write_startup_config_to_path,
    ConfigSignatureConfig, LoadedStartupConfig, StartupConfig, CONFIG_SCHEMA_VERSION,
};
use crate::security::diagnostics::{run_local_self_check, CheckStatus, LocalSelfCheckReport};
use crate::security::hardening::{detect_hardening_status, HardeningStatus};
use crate::security::lsm::{detect_lsm_status, LsmStatus};

const TUI_AUDIT_CAP: usize = 250;
const MAX_INPUT_BUFFER_LEN: usize = 4096;
const MAX_AUDIT_SEARCH_LEN: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorTuiEntry {
    Dashboard,
    Setup,
    Diagnostics,
    Audit,
}

#[derive(Debug, Clone)]
pub struct OperatorTuiParams {
    pub startup: LoadedStartupConfig,
    pub startup_created: bool,
    pub audit_limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorTuiResult {
    pub saved_config: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Dashboard,
    Setup,
    Diagnostics,
    Audit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupPage {
    Detector,
    Bot,
    Audit,
    Diagnostics,
    Integrity,
    Runtime,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupField {
    DetectorBackend,
    TargetReadings,
    LiteralSequencePatterns,
    SpecialPhrases,
    StampText,
    SendTemplate,
    ReactionEmojiId,
    ReactionEmojiName,
    ReactionAnimated,
    MaxCountCap,
    MaxSendChars,
    ActionPolicy,
    AuditSqlitePath,
    AuditExportMaxRows,
    AuditQueryMaxRows,
    PseudoIdHmacKeyEnv,
    SignatureEnabled,
    SignaturePath,
    SignatureHmacEnv,
    LocalSelfCheck,
    VerifyHardeningArtifacts,
    VerifyGeneratedArtifacts,
    DependencySecurityCheckMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Normal,
    SetupEdit(SetupField),
    AuditSearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuditSort {
    NewestFirst,
    OldestFirst,
    EventType,
    Mode,
}

#[derive(Debug, Clone)]
struct SetupState {
    draft: StartupConfig,
    defaults: StartupConfig,
    page: SetupPage,
    selected: usize,
}

#[derive(Debug, Clone)]
struct AuditState {
    rows: Vec<AuditEventRow>,
    stats: AuditStats,
    status: String,
    search: String,
    sort: AuditSort,
    mode_filter: Option<String>,
}

#[derive(Debug, Clone)]
struct DiagnosticsSummary {
    local_self_check: LocalSelfCheckReport,
    audit_db_health: String,
    dependency_snapshot_status: String,
    integrity_verify_result: String,
    export_safe_policy_status: String,
    confinement_state: String,
    writable_paths: Vec<String>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone)]
struct OperatorTuiApp {
    startup: LoadedStartupConfig,
    startup_created: bool,
    saved_config: bool,
    screen: Screen,
    input_mode: InputMode,
    input_buffer: String,
    status: String,
    lsm_status: LsmStatus,
    hardening_status: HardeningStatus,
    diagnostics: DiagnosticsSummary,
    setup: SetupState,
    audit: AuditState,
}

const DETECTOR_FIELDS: &[SetupField] = &[
    SetupField::DetectorBackend,
    SetupField::TargetReadings,
    SetupField::LiteralSequencePatterns,
    SetupField::SpecialPhrases,
];

const BOT_FIELDS: &[SetupField] = &[
    SetupField::StampText,
    SetupField::SendTemplate,
    SetupField::ReactionEmojiId,
    SetupField::ReactionEmojiName,
    SetupField::ReactionAnimated,
    SetupField::MaxCountCap,
    SetupField::MaxSendChars,
    SetupField::ActionPolicy,
];

const AUDIT_FIELDS: &[SetupField] = &[
    SetupField::AuditSqlitePath,
    SetupField::AuditExportMaxRows,
    SetupField::AuditQueryMaxRows,
    SetupField::PseudoIdHmacKeyEnv,
    SetupField::SignatureEnabled,
    SetupField::SignaturePath,
    SetupField::SignatureHmacEnv,
];

const DIAGNOSTIC_FIELDS: &[SetupField] = &[
    SetupField::LocalSelfCheck,
    SetupField::VerifyHardeningArtifacts,
    SetupField::VerifyGeneratedArtifacts,
    SetupField::DependencySecurityCheckMode,
];

const INTEGRITY_FIELDS: &[SetupField] = &[
    SetupField::SignatureEnabled,
    SetupField::SignaturePath,
    SetupField::SignatureHmacEnv,
];

pub fn run_operator_tui(entry: OperatorTuiEntry, params: OperatorTuiParams) -> Result<OperatorTuiResult, String> {
    let lsm_status = detect_lsm_status();
    let hardening_status = detect_hardening_status();
    let local_self_check = run_local_self_check(&params.startup);
    let diagnostics = build_diagnostics_summary(&params.startup, &local_self_check, &lsm_status, &hardening_status);
    let audit = load_audit_state(&params.startup, params.audit_limit);

    let mut app = OperatorTuiApp {
        status: if params.startup_created {
            "new config created from yaml defaults. open setup to review before first run".to_string()
        } else {
            "1:dashboard 2:setup 3:diagnostics 4:audit   q:quit".to_string()
        },
        startup_created: params.startup_created,
        saved_config: false,
        screen: match entry {
            OperatorTuiEntry::Dashboard => Screen::Dashboard,
            OperatorTuiEntry::Setup => Screen::Setup,
            OperatorTuiEntry::Diagnostics => Screen::Diagnostics,
            OperatorTuiEntry::Audit => Screen::Audit,
        },
        input_mode: InputMode::Normal,
        input_buffer: String::new(),
        lsm_status,
        hardening_status,
        diagnostics,
        setup: SetupState {
            draft: params.startup.app.clone(),
            defaults: canonical_startup_config(),
            page: if params.startup_created { SetupPage::Detector } else { SetupPage::Preview },
            selected: 0,
        },
        audit,
        startup: params.startup,
    };

    enable_raw_mode().map_err(|err| err.to_string())?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|err| err.to_string())?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|err| err.to_string())?;

    let loop_result = (|| -> Result<OperatorTuiResult, String> {
        loop {
            terminal
                .draw(|frame| render_app(frame, &app))
                .map_err(|err| err.to_string())?;

            if !event::poll(std::time::Duration::from_millis(200)).map_err(|err| err.to_string())? {
                continue;
            }

            let Event::Key(key) = event::read().map_err(|err| err.to_string())? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match app.input_mode {
                InputMode::Normal => {
                    if handle_global_keys(&mut app, key.code)? {
                        return Ok(OperatorTuiResult { saved_config: app.saved_config });
                    }
                }
                InputMode::SetupEdit(field) => {
                    if handle_setup_edit_keys(&mut app, field, key.code)? {
                        app.input_mode = InputMode::Normal;
                    }
                }
                InputMode::AuditSearch => {
                    if handle_audit_search_keys(&mut app, key.code)? {
                        app.input_mode = InputMode::Normal;
                    }
                }
            }
        }
    })();

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    loop_result
}

fn render_app(frame: &mut Frame<'_>, app: &OperatorTuiApp) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(12), Constraint::Length(3)])
        .split(frame.area());

    let header = Paragraph::new(format!(
        "oo-bot operator tui   active={}   config={}   fingerprint={}   screen={}",
        app.lsm_status.active_lsm_summary(),
        app.startup.config_path.display(),
        truncate_middle(&app.startup.config_fingerprint, 18),
        match app.screen {
            Screen::Dashboard => "dashboard",
            Screen::Setup => "setup",
            Screen::Diagnostics => "diagnostics",
            Screen::Audit => "audit",
        }
    ))
    .block(Block::default().title("Status").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, layout[0]);

    match app.screen {
        Screen::Dashboard => render_dashboard(frame, layout[1], app),
        Screen::Setup => render_setup(frame, layout[1], app),
        Screen::Diagnostics => render_diagnostics(frame, layout[1], app),
        Screen::Audit => render_audit(frame, layout[1], app),
    }

    let footer_text = match app.input_mode {
        InputMode::Normal => app.status.clone(),
        InputMode::SetupEdit(field) => {
            let default = setup_default_value(&app.setup.defaults, field);
            format!(
                "editing {}   enter=commit custom value   esc=cancel   blank enter => default ({default})",
                setup_field_label(field)
            )
        }
        InputMode::AuditSearch => "audit search: type to filter, enter=apply, esc=cancel".to_string(),
    };
    let footer = Paragraph::new(footer_text)
    .block(Block::default().title("Help").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(footer, layout[2]);
}

fn render_dashboard(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &OperatorTuiApp) {
    let chunks = if area.width < 100 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(10), Constraint::Length(10), Constraint::Min(8)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(34), Constraint::Percentage(33), Constraint::Percentage(33)])
            .split(area)
    };

    let system = Paragraph::new(format!(
        "config schema: {}\naudit schema: {}\nconfig fingerprint: {}\ndetector backend: {:?}\nactive LSM: {}\nconfinement: {}\nhardening: {}\nstartup mode: {}",
        CONFIG_SCHEMA_VERSION,
        SCHEMA_VERSION,
        app.startup.config_fingerprint,
        app.startup.app.detector.backend,
        app.lsm_status.active_lsm_summary(),
        app.diagnostics.confinement_state,
        app.hardening_status.summary(),
        if app.startup_created { "fresh bootstrap" } else { "existing config" },
    ))
    .block(Block::default().title("Dashboard").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(system, chunks[0]);

    let diagnostics = Paragraph::new(format!(
        "local self-check: {}\naudit db health: {}\nintegrity verify: {}\ndependency snapshot: {}\nexport-safe policy: {}\nstartup config: {}\n",
        check_status_label(app.diagnostics.local_self_check.healthy),
        app.diagnostics.audit_db_health,
        app.diagnostics.integrity_verify_result,
        app.diagnostics.dependency_snapshot_status,
        app.diagnostics.export_safe_policy_status,
        app.startup.config_path.display(),
    ))
    .block(Block::default().title("Health").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(diagnostics, chunks[1]);

    let recommendations = Paragraph::new(format!(
        "writable paths:\n{}\n\nrecommendations:\n{}",
        app.diagnostics.writable_paths.join("\n"),
        app.diagnostics.recommendations.join("\n")
    ))
    .block(Block::default().title("Runtime Env").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(recommendations, chunks[2]);
}

fn render_setup(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &OperatorTuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10), Constraint::Length(6)])
        .split(area);

    let page_title = Paragraph::new(format!(
        "page={}   left/right=change page   up/down=select   enter=apply default   e=edit custom   space=cycle   p=preview   s=save on preview",
        setup_page_name(app.setup.page)
    ))
    .block(Block::default().title("Setup Wizard").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(page_title, chunks[0]);

    if app.setup.page == SetupPage::Preview {
        let preview = Paragraph::new(build_setup_preview(app))
            .block(Block::default().title("Preview").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(preview, chunks[1]);
    } else if app.setup.page == SetupPage::Runtime {
        let runtime = Paragraph::new(build_runtime_environment_block(app))
            .block(Block::default().title("Runtime Environment").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(runtime, chunks[1]);
    } else {
        let body = Paragraph::new(build_setup_field_block(app))
            .block(Block::default().title("Fields").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(body, chunks[1]);
    }

    let validation = Paragraph::new(build_setup_validation_block(app))
        .block(Block::default().title("Validation").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(validation, chunks[2]);
}

fn render_diagnostics(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &OperatorTuiApp) {
    let chunks = if area.width < 100 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(12), Constraint::Min(10)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
    };

    let summary = Paragraph::new(format!(
        "active LSM: {}\ncurrent confinement: {}\nhardening status: {}\nconfig fingerprint: {}\ndetector backend: {:?}\naudit DB health: {}\nschema version: {}\nintegrity verify result: {}\nexport-safe policy: {}\ndependency audit snapshot: {}",
        app.lsm_status.active_lsm_summary(),
        app.diagnostics.confinement_state,
        app.hardening_status.summary(),
        app.startup.config_fingerprint,
        app.startup.app.detector.backend,
        app.diagnostics.audit_db_health,
        CONFIG_SCHEMA_VERSION,
        app.diagnostics.integrity_verify_result,
        app.diagnostics.export_safe_policy_status,
        app.diagnostics.dependency_snapshot_status,
    ))
    .block(Block::default().title("Diagnostics Summary").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, chunks[0]);

    let items = app
        .diagnostics
        .local_self_check
        .items
        .iter()
        .map(|item| format!("[{}] {}: {}", format_check_status(&item.status), item.name, item.detail))
        .collect::<Vec<_>>()
        .join("\n");
    let details = Paragraph::new(items)
        .block(Block::default().title("Self-check Details").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(details, chunks[1]);
}

fn render_audit(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &OperatorTuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Length(6), Constraint::Min(10)])
        .split(area);

    let filtered = filtered_audit_rows(&app.audit);
    let summary = Paragraph::new(format!(
        "rows loaded: {}   rows shown: {}   search: {:?}   sort: {:?}   mode filter: {:?}\nbackend comparison: {}\nsuppression reasons: {}\nmode transitions: {}",
        app.audit.rows.len(),
        filtered.len(),
        if app.audit.search.is_empty() { None } else { Some(app.audit.search.as_str()) },
        app.audit.sort,
        app.audit.mode_filter.as_deref(),
        format_counts(&app.audit.stats.by_backend),
        format_counts(&app.audit.stats.by_suppressed_reason),
        filtered.iter().filter(|row| row.event_type == "mode_changed").count(),
    ))
    .block(Block::default().title("Audit Browser").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, chunks[0]);

    let status = Paragraph::new(format!(
        "{}\nkeys: /=search  o=sort  m=mode filter  r=refresh  q=quit",
        app.audit.status
    ))
    .block(Block::default().title("Audit Controls").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(status, chunks[1]);

    let lines = filtered
        .iter()
        .take(TUI_AUDIT_CAP)
        .map(|row| {
            format!(
                "#{} {} backend={} action={} reason={} mode={}",
                row.event_id,
                row.event_type,
                row.detector_backend,
                row.selected_action,
                if row.suppressed_reason.is_empty() { "-" } else { row.suppressed_reason.as_str() },
                row.mode,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let rows = Paragraph::new(if lines.is_empty() { "no rows match the current filter".to_string() } else { lines })
        .block(Block::default().title("Audit Rows").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(rows, chunks[2]);
}

fn handle_global_keys(app: &mut OperatorTuiApp, code: KeyCode) -> Result<bool, String> {
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('1') => app.screen = Screen::Dashboard,
        KeyCode::Char('2') => app.screen = Screen::Setup,
        KeyCode::Char('3') => app.screen = Screen::Diagnostics,
        KeyCode::Char('4') => app.screen = Screen::Audit,
        _ => {}
    }

    match app.screen {
        Screen::Setup => handle_setup_normal_keys(app, code)?,
        Screen::Audit => handle_audit_normal_keys(app, code)?,
        Screen::Dashboard | Screen::Diagnostics => {}
    }

    Ok(false)
}

fn handle_setup_normal_keys(app: &mut OperatorTuiApp, code: KeyCode) -> Result<(), String> {
    match code {
        KeyCode::Left => {
            app.setup.page = previous_setup_page(app.setup.page);
            app.setup.selected = 0;
        }
        KeyCode::Right => {
            app.setup.page = next_setup_page(app.setup.page);
            app.setup.selected = 0;
        }
        KeyCode::Char('p') => {
            app.setup.page = SetupPage::Preview;
        }
        KeyCode::Up => {
            if app.setup.selected > 0 {
                app.setup.selected -= 1;
            }
        }
        KeyCode::Down => {
            let len = setup_fields_for_page(app.setup.page).len();
            if len > 0 && app.setup.selected + 1 < len {
                app.setup.selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(field) = selected_setup_field(&app.setup) {
                restore_default_for_field(&mut app.setup, field);
                app.status = format!("restored default for {}", setup_field_label(field));
            }
        }
        KeyCode::Char('e') => {
            if let Some(field) = selected_setup_field(&app.setup) {
                app.input_mode = InputMode::SetupEdit(field);
                app.input_buffer.clear();
            }
        }
        KeyCode::Char(' ') => {
            if let Some(field) = selected_setup_field(&app.setup) {
                cycle_field_value(&mut app.setup, field);
            }
        }
        KeyCode::Char('s') => {
            if app.setup.page != SetupPage::Preview {
                app.status = "move to preview before saving".to_string();
            } else {
                validate_startup_config(&app.setup.draft).map_err(|err| err.to_string())?;
                write_startup_config_to_path(&app.startup.config_path, &app.setup.draft)
                    .map_err(|err| err.to_string())?;
                app.startup = crate::config::load_startup_config_from_path(&app.startup.config_path)
                    .map_err(|err| err.to_string())?;
                let local_self_check = run_local_self_check(&app.startup);
                app.diagnostics = build_diagnostics_summary(
                    &app.startup,
                    &local_self_check,
                    &app.lsm_status,
                    &app.hardening_status,
                );
                app.audit = load_audit_state(&app.startup, TUI_AUDIT_CAP);
                app.saved_config = true;
                app.status = format!("saved {}", app.startup.config_path.display());
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_setup_edit_keys(app: &mut OperatorTuiApp, field: SetupField, code: KeyCode) -> Result<bool, String> {
    match code {
        KeyCode::Esc => return Ok(true),
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Enter => {
            apply_custom_field_input(&mut app.setup, field, &app.input_buffer)?;
            app.status = format!("updated {}", setup_field_label(field));
            app.input_buffer.clear();
            return Ok(true);
        }
        KeyCode::Char(ch) => {
            if app.input_buffer.len() < MAX_INPUT_BUFFER_LEN {
                app.input_buffer.push(ch);
            }
        }
        _ => {}
    }

    Ok(false)
}

fn handle_audit_normal_keys(app: &mut OperatorTuiApp, code: KeyCode) -> Result<(), String> {
    match code {
        KeyCode::Char('/') => {
            app.input_mode = InputMode::AuditSearch;
            app.input_buffer = app.audit.search.clone();
        }
        KeyCode::Char('o') => {
            app.audit.sort = match app.audit.sort {
                AuditSort::NewestFirst => AuditSort::OldestFirst,
                AuditSort::OldestFirst => AuditSort::EventType,
                AuditSort::EventType => AuditSort::Mode,
                AuditSort::Mode => AuditSort::NewestFirst,
            };
        }
        KeyCode::Char('m') => {
            app.audit.mode_filter = match app.audit.mode_filter.as_deref() {
                None => Some("normal".to_string()),
                Some("normal") => Some("observe_only".to_string()),
                Some("observe_only") => Some("react_only".to_string()),
                Some("react_only") => Some("audit_only".to_string()),
                Some("audit_only") => Some("full_disable".to_string()),
                _ => None,
            };
        }
        KeyCode::Char('r') => {
            app.audit = load_audit_state(&app.startup, TUI_AUDIT_CAP);
        }
        _ => {}
    }
    Ok(())
}

fn handle_audit_search_keys(app: &mut OperatorTuiApp, code: KeyCode) -> Result<bool, String> {
    match code {
        KeyCode::Esc => return Ok(true),
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Enter => {
            app.audit.search = app
                .input_buffer
                .trim()
                .chars()
                .take(MAX_AUDIT_SEARCH_LEN)
                .collect();
            app.status = format!("updated audit search: {:?}", app.audit.search);
            return Ok(true);
        }
        KeyCode::Char(ch) => {
            if app.input_buffer.len() < MAX_INPUT_BUFFER_LEN {
                app.input_buffer.push(ch);
            }
        }
        _ => {}
    }
    Ok(false)
}

fn build_diagnostics_summary(
    startup: &LoadedStartupConfig,
    local_self_check: &LocalSelfCheckReport,
    lsm_status: &LsmStatus,
    hardening_status: &HardeningStatus,
) -> DiagnosticsSummary {
    let audit_db_health = local_self_check
        .items
        .iter()
        .find(|item| item.name == "audit_db_health")
        .map(|item| item.detail.clone())
        .unwrap_or_else(|| "not checked".to_string());

    DiagnosticsSummary {
        local_self_check: local_self_check.clone(),
        audit_db_health,
        dependency_snapshot_status: dependency_snapshot_status(&startup.app.diagnostics.security_snapshot_path),
        integrity_verify_result: if startup.app.integrity.config_signature.is_some() {
            "detached signature verified on load".to_string()
        } else {
            "unsigned config".to_string()
        },
        export_safe_policy_status: format!(
            "export cap={} query cap={} pseudo-id={}",
            startup.app.audit.export_max_rows,
            startup.app.audit.query_max_rows,
            if startup.pseudo_id_hmac_key.is_some() { "enabled" } else { "disabled" }
        ),
        confinement_state: confinement_state(lsm_status, hardening_status),
        writable_paths: vec![
            writable_parent_label("config", &startup.config_path),
            writable_parent_label("audit", &startup.app.audit.sqlite_path),
            writable_parent_label("security snapshot", &startup.app.diagnostics.security_snapshot_path),
        ],
        recommendations: runtime_recommendations(lsm_status, hardening_status),
    }
}

fn load_audit_state(startup: &LoadedStartupConfig, audit_limit: usize) -> AuditState {
    let cfg = AuditStoreConfig {
        sqlite_path: startup.app.audit.sqlite_path.clone(),
        busy_timeout_ms: startup.app.audit.busy_timeout_ms,
        export_max_rows: startup.app.audit.export_max_rows,
        query_max_rows: startup.app.audit.query_max_rows,
    };

    if !cfg.sqlite_path.exists() {
        return AuditState {
            rows: Vec::new(),
            stats: AuditStats {
                total: 0,
                by_event_type: BTreeMap::new(),
                by_backend: BTreeMap::new(),
                by_suppressed_reason: BTreeMap::new(),
                by_mode: BTreeMap::new(),
            },
            status: "audit db does not exist yet; browser is read-only and idle".to_string(),
            search: String::new(),
            sort: AuditSort::NewestFirst,
            mode_filter: None,
        };
    }

    let store = match AuditStore::open_ro(&cfg) {
        Ok(store) => store,
        Err(err) => {
            return AuditState {
                rows: Vec::new(),
                stats: AuditStats {
                    total: 0,
                    by_event_type: BTreeMap::new(),
                    by_backend: BTreeMap::new(),
                    by_suppressed_reason: BTreeMap::new(),
                    by_mode: BTreeMap::new(),
                },
                status: format!("failed to open audit db read-only: {err}"),
                search: String::new(),
                sort: AuditSort::NewestFirst,
                mode_filter: None,
            };
        }
    };

    let filter = AuditQueryFilter {
        limit: Some(audit_limit.min(TUI_AUDIT_CAP).min(startup.app.audit.query_max_rows)),
        ..AuditQueryFilter::default()
    };

    let rows = store.tail(&filter).unwrap_or_default();
    let stats = store.stats(&filter).unwrap_or(AuditStats {
        total: rows.len(),
        by_event_type: BTreeMap::new(),
        by_backend: BTreeMap::new(),
        by_suppressed_reason: BTreeMap::new(),
        by_mode: BTreeMap::new(),
    });

    AuditState {
        rows,
        stats,
        status: "audit browser uses read-only sqlite and caps visible rows".to_string(),
        search: String::new(),
        sort: AuditSort::NewestFirst,
        mode_filter: None,
    }
}

fn build_setup_field_block(app: &OperatorTuiApp) -> String {
    let mut out = vec![format!(
        "{}\n",
        setup_page_help(app.setup.page)
    )];

    for (index, field) in setup_fields_for_page(app.setup.page).iter().enumerate() {
        let marker = if index == app.setup.selected { ">" } else { " " };
        out.push(format!(
            "{marker} {} = {}\n  default: {}\n",
            setup_field_label(*field),
            setup_field_value(&app.setup.draft, *field),
            setup_default_value(&app.setup.defaults, *field)
        ));
    }

    out.join("\n")
}

fn build_runtime_environment_block(app: &OperatorTuiApp) -> String {
    format!(
        "detected LSM: {}\nhardening status: {}\nwritable paths:\n{}\n\nservice/container recommendations:\n{}",
        app.lsm_status.active_lsm_summary(),
        app.hardening_status.summary(),
        app.diagnostics.writable_paths.join("\n"),
        app.diagnostics.recommendations.join("\n"),
    )
}

fn build_setup_preview(app: &OperatorTuiApp) -> String {
    let yaml = crate::config::render_startup_config_yaml(&app.setup.draft)
        .unwrap_or_else(|err| format!("# render failed: {err}\n"));
    format!("strict config preview:\n\n{yaml}")
}

fn build_setup_validation_block(app: &OperatorTuiApp) -> String {
    let mut lines = Vec::new();

    match validate_startup_config(&app.setup.draft) {
        Ok(()) => lines.push("[pass] strict schema validation passed".to_string()),
        Err(err) => lines.push(format!("[fail] strict schema validation failed: {err}")),
    }

    if let Some(signature) = &app.setup.draft.integrity.config_signature {
        lines.push(format!(
            "[info] signature path={} key env={} env_present={}",
            signature.detached_hmac_sha256_path.display(),
            signature.hmac_key_env,
            std::env::var(&signature.hmac_key_env).is_ok()
        ));
    } else {
        lines.push("[info] detached config signature is disabled".to_string());
    }

    lines.join("\n")
}

fn dependency_snapshot_status(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|value| value.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            format!("present at {} (mtime unix={modified})", path.display())
        }
        Err(_) => format!("missing at {}", path.display()),
    }
}

fn confinement_state(lsm_status: &LsmStatus, hardening_status: &HardeningStatus) -> String {
    if let Some(profile) = &lsm_status.apparmor_profile {
        return format!("AppArmor profile {profile}");
    }
    if let Some(context) = &lsm_status.selinux_context {
        return format!("SELinux context {context}");
    }
    if lsm_status.active_modules.is_empty() {
        return format!("unconfined or undetected; {}", hardening_status.summary());
    }
    format!("lsm={}, {}", lsm_status.active_lsm_summary(), hardening_status.summary())
}

fn writable_parent_label(label: &str, path: &Path) -> String {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    format!("{label}: {}", parent.display())
}

fn runtime_recommendations(lsm_status: &LsmStatus, hardening_status: &HardeningStatus) -> Vec<String> {
    let mut out = Vec::new();
    if lsm_status.major_lsm.is_none() {
        out.push("- prefer running under AppArmor or SELinux confinement".to_string());
    }
    if !hardening_status.hardened_x64_requested {
        out.push("- consider hardened-x64 release profile for Linux x86_64 deployments".to_string());
    }
    out.push("- mount state directories writable but keep source tree read-only in service/container".to_string());
    out.push("- keep OO_CONFIG_PATH and audit sqlite on persistent storage".to_string());
    out
}

fn filtered_audit_rows(audit: &AuditState) -> Vec<AuditEventRow> {
    let mut rows = audit
        .rows
        .iter()
        .filter(|row| {
            let search_ok = if audit.search.trim().is_empty() {
                true
            } else {
                let haystack = format!(
                    "{} {} {} {} {}",
                    row.event_type, row.detector_backend, row.selected_action, row.suppressed_reason, row.mode
                )
                .to_ascii_lowercase();
                haystack.contains(&audit.search.to_ascii_lowercase())
            };
            let mode_ok = match &audit.mode_filter {
                Some(mode) => &row.mode == mode,
                None => true,
            };
            search_ok && mode_ok
        })
        .cloned()
        .collect::<Vec<_>>();

    match audit.sort {
        AuditSort::NewestFirst => rows.sort_by(|a, b| b.event_id.cmp(&a.event_id)),
        AuditSort::OldestFirst => rows.sort_by(|a, b| a.event_id.cmp(&b.event_id)),
        AuditSort::EventType => rows.sort_by(|a, b| a.event_type.cmp(&b.event_type).then(a.event_id.cmp(&b.event_id))),
        AuditSort::Mode => rows.sort_by(|a, b| a.mode.cmp(&b.mode).then(a.event_id.cmp(&b.event_id))),
    }

    rows
}

fn format_counts(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn previous_setup_page(page: SetupPage) -> SetupPage {
    match page {
        SetupPage::Detector => SetupPage::Preview,
        SetupPage::Bot => SetupPage::Detector,
        SetupPage::Audit => SetupPage::Bot,
        SetupPage::Diagnostics => SetupPage::Audit,
        SetupPage::Integrity => SetupPage::Diagnostics,
        SetupPage::Runtime => SetupPage::Integrity,
        SetupPage::Preview => SetupPage::Runtime,
    }
}

fn next_setup_page(page: SetupPage) -> SetupPage {
    match page {
        SetupPage::Detector => SetupPage::Bot,
        SetupPage::Bot => SetupPage::Audit,
        SetupPage::Audit => SetupPage::Diagnostics,
        SetupPage::Diagnostics => SetupPage::Integrity,
        SetupPage::Integrity => SetupPage::Runtime,
        SetupPage::Runtime => SetupPage::Preview,
        SetupPage::Preview => SetupPage::Detector,
    }
}

fn setup_page_name(page: SetupPage) -> &'static str {
    match page {
        SetupPage::Detector => "detector",
        SetupPage::Bot => "bot",
        SetupPage::Audit => "audit",
        SetupPage::Diagnostics => "diagnostics",
        SetupPage::Integrity => "integrity",
        SetupPage::Runtime => "runtime environment",
        SetupPage::Preview => "preview",
    }
}

fn setup_page_help(page: SetupPage) -> &'static str {
    match page {
        SetupPage::Detector => "detector defaults and matching behavior",
        SetupPage::Bot => "bot response formatting and outbound caps",
        SetupPage::Audit => "audit storage, export caps, and pseudo-id policy",
        SetupPage::Diagnostics => "startup checks and build/security verification policy",
        SetupPage::Integrity => "detached signature verification and signing guidance",
        SetupPage::Runtime => "detected environment, writable paths, and deployment guidance",
        SetupPage::Preview => "strict yaml preview and validation before write",
    }
}

fn setup_fields_for_page(page: SetupPage) -> &'static [SetupField] {
    match page {
        SetupPage::Detector => DETECTOR_FIELDS,
        SetupPage::Bot => BOT_FIELDS,
        SetupPage::Audit => AUDIT_FIELDS,
        SetupPage::Diagnostics => DIAGNOSTIC_FIELDS,
        SetupPage::Integrity => INTEGRITY_FIELDS,
        SetupPage::Runtime | SetupPage::Preview => &[],
    }
}

fn selected_setup_field(setup: &SetupState) -> Option<SetupField> {
    setup_fields_for_page(setup.page).get(setup.selected).copied()
}

fn restore_default_for_field(setup: &mut SetupState, field: SetupField) {
    match field {
        SetupField::DetectorBackend => setup.draft.detector.backend = setup.defaults.detector.backend,
        SetupField::TargetReadings => setup.draft.detector.target_readings = setup.defaults.detector.target_readings.clone(),
        SetupField::LiteralSequencePatterns => setup.draft.detector.literal_sequence_patterns = setup.defaults.detector.literal_sequence_patterns.clone(),
        SetupField::SpecialPhrases => setup.draft.detector.special_phrases = setup.defaults.detector.special_phrases.clone(),
        SetupField::StampText => setup.draft.bot.stamp_text = setup.defaults.bot.stamp_text.clone(),
        SetupField::SendTemplate => setup.draft.bot.send_template = setup.defaults.bot.send_template.clone(),
        SetupField::ReactionEmojiId => setup.draft.bot.reaction.emoji_id = setup.defaults.bot.reaction.emoji_id,
        SetupField::ReactionEmojiName => setup.draft.bot.reaction.emoji_name = setup.defaults.bot.reaction.emoji_name.clone(),
        SetupField::ReactionAnimated => setup.draft.bot.reaction.animated = setup.defaults.bot.reaction.animated,
        SetupField::MaxCountCap => setup.draft.bot.max_count_cap = setup.defaults.bot.max_count_cap,
        SetupField::MaxSendChars => setup.draft.bot.max_send_chars = setup.defaults.bot.max_send_chars,
        SetupField::ActionPolicy => setup.draft.bot.action_policy = setup.defaults.bot.action_policy.clone(),
        SetupField::AuditSqlitePath => setup.draft.audit.sqlite_path = setup.defaults.audit.sqlite_path.clone(),
        SetupField::AuditExportMaxRows => setup.draft.audit.export_max_rows = setup.defaults.audit.export_max_rows,
        SetupField::AuditQueryMaxRows => setup.draft.audit.query_max_rows = setup.defaults.audit.query_max_rows,
        SetupField::PseudoIdHmacKeyEnv => setup.draft.integrity.pseudo_id_hmac_key_env = setup.defaults.integrity.pseudo_id_hmac_key_env.clone(),
        SetupField::SignatureEnabled => setup.draft.integrity.config_signature = setup.defaults.integrity.config_signature.clone(),
        SetupField::SignaturePath => {
            setup.draft.integrity.config_signature = setup.defaults.integrity.config_signature.clone();
        }
        SetupField::SignatureHmacEnv => {
            setup.draft.integrity.config_signature = setup.defaults.integrity.config_signature.clone();
        }
        SetupField::LocalSelfCheck => setup.draft.diagnostics.local_self_check_on_startup = setup.defaults.diagnostics.local_self_check_on_startup,
        SetupField::VerifyHardeningArtifacts => {
            setup.draft.diagnostics.verify_hardening_artifacts = setup.defaults.diagnostics.verify_hardening_artifacts;
        }
        SetupField::VerifyGeneratedArtifacts => {
            setup.draft.diagnostics.verify_generated_artifacts = setup.defaults.diagnostics.verify_generated_artifacts;
        }
        SetupField::DependencySecurityCheckMode => {
            setup.draft.diagnostics.dependency_security_check_mode =
                setup.defaults.diagnostics.dependency_security_check_mode.clone();
        }
    }
}

fn cycle_field_value(setup: &mut SetupState, field: SetupField) {
    match field {
        SetupField::ReactionAnimated => {
            setup.draft.bot.reaction.animated = !setup.draft.bot.reaction.animated;
        }
        SetupField::ActionPolicy => {
            setup.draft.bot.action_policy = match setup.draft.bot.action_policy {
                crate::app::analyze_message::ActionPolicy::ReactOrSend => crate::app::analyze_message::ActionPolicy::ReactOnly,
                crate::app::analyze_message::ActionPolicy::ReactOnly => crate::app::analyze_message::ActionPolicy::NoOutbound,
                crate::app::analyze_message::ActionPolicy::NoOutbound => crate::app::analyze_message::ActionPolicy::ReactOrSend,
            };
        }
        SetupField::LocalSelfCheck => {
            setup.draft.diagnostics.local_self_check_on_startup =
                !setup.draft.diagnostics.local_self_check_on_startup;
        }
        SetupField::VerifyHardeningArtifacts => {
            setup.draft.diagnostics.verify_hardening_artifacts =
                !setup.draft.diagnostics.verify_hardening_artifacts;
        }
        SetupField::VerifyGeneratedArtifacts => {
            setup.draft.diagnostics.verify_generated_artifacts =
                !setup.draft.diagnostics.verify_generated_artifacts;
        }
        SetupField::DependencySecurityCheckMode => {
            setup.draft.diagnostics.dependency_security_check_mode =
                match setup.draft.diagnostics.dependency_security_check_mode {
                    crate::config::DependencySecurityCheckMode::Disabled => crate::config::DependencySecurityCheckMode::OfflineSnapshot,
                    crate::config::DependencySecurityCheckMode::OfflineSnapshot => crate::config::DependencySecurityCheckMode::Disabled,
                };
        }
        SetupField::SignatureEnabled => {
            setup.draft.integrity.config_signature = if setup.draft.integrity.config_signature.is_some() {
                None
            } else {
                Some(ConfigSignatureConfig {
                    detached_hmac_sha256_path: PathBuf::from("config/oo-bot.yaml.sig"),
                    hmac_key_env: "OO_CONFIG_HMAC_KEY".to_string(),
                })
            };
        }
        _ => {}
    }
}

fn apply_custom_field_input(setup: &mut SetupState, field: SetupField, raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        restore_default_for_field(setup, field);
        return Ok(());
    }

    match field {
        SetupField::DetectorBackend => {
            setup.draft.detector.backend = serde_yaml::from_str(trimmed).map_err(|err| err.to_string())?;
        }
        SetupField::TargetReadings => setup.draft.detector.target_readings = parse_csv(trimmed),
        SetupField::LiteralSequencePatterns => {
            setup.draft.detector.literal_sequence_patterns = parse_csv(trimmed)
        }
        SetupField::SpecialPhrases => setup.draft.detector.special_phrases = parse_csv(trimmed),
        SetupField::StampText => setup.draft.bot.stamp_text = trimmed.to_string(),
        SetupField::SendTemplate => setup.draft.bot.send_template = trimmed.to_string(),
        SetupField::ReactionEmojiId => {
            setup.draft.bot.reaction.emoji_id =
                trimmed.parse::<u64>().map_err(|_| "emoji_id must be a u64".to_string())?;
        }
        SetupField::ReactionEmojiName => setup.draft.bot.reaction.emoji_name = trimmed.to_string(),
        SetupField::ReactionAnimated => {}
        SetupField::MaxCountCap => {
            setup.draft.bot.max_count_cap =
                trimmed.parse::<usize>().map_err(|_| "max_count_cap must be usize".to_string())?;
        }
        SetupField::MaxSendChars => {
            setup.draft.bot.max_send_chars =
                trimmed.parse::<usize>().map_err(|_| "max_send_chars must be usize".to_string())?;
        }
        SetupField::ActionPolicy => {}
        SetupField::AuditSqlitePath => setup.draft.audit.sqlite_path = PathBuf::from(trimmed),
        SetupField::AuditExportMaxRows => {
            setup.draft.audit.export_max_rows =
                trimmed.parse::<usize>().map_err(|_| "export_max_rows must be usize".to_string())?;
        }
        SetupField::AuditQueryMaxRows => {
            setup.draft.audit.query_max_rows =
                trimmed.parse::<usize>().map_err(|_| "query_max_rows must be usize".to_string())?;
        }
        SetupField::PseudoIdHmacKeyEnv => {
            setup.draft.integrity.pseudo_id_hmac_key_env = Some(trimmed.to_string());
        }
        SetupField::SignatureEnabled => {}
        SetupField::SignaturePath => {
            ensure_signature_config(&mut setup.draft);
            if let Some(signature) = setup.draft.integrity.config_signature.as_mut() {
                signature.detached_hmac_sha256_path = PathBuf::from(trimmed);
            }
        }
        SetupField::SignatureHmacEnv => {
            ensure_signature_config(&mut setup.draft);
            if let Some(signature) = setup.draft.integrity.config_signature.as_mut() {
                signature.hmac_key_env = trimmed.to_string();
            }
        }
        SetupField::LocalSelfCheck => {}
        SetupField::VerifyHardeningArtifacts => {}
        SetupField::VerifyGeneratedArtifacts => {}
        SetupField::DependencySecurityCheckMode => {}
    }

    Ok(())
}

fn ensure_signature_config(config: &mut StartupConfig) {
    if config.integrity.config_signature.is_none() {
        config.integrity.config_signature = Some(ConfigSignatureConfig {
            detached_hmac_sha256_path: PathBuf::from("config/oo-bot.yaml.sig"),
            hmac_key_env: "OO_CONFIG_HMAC_KEY".to_string(),
        });
    }
}

fn setup_field_label(field: SetupField) -> &'static str {
    match field {
        SetupField::DetectorBackend => "detector.backend",
        SetupField::TargetReadings => "detector.target_readings",
        SetupField::LiteralSequencePatterns => "detector.literal_sequence_patterns",
        SetupField::SpecialPhrases => "detector.special_phrases",
        SetupField::StampText => "bot.stamp_text",
        SetupField::SendTemplate => "bot.send_template",
        SetupField::ReactionEmojiId => "bot.reaction.emoji_id",
        SetupField::ReactionEmojiName => "bot.reaction.emoji_name",
        SetupField::ReactionAnimated => "bot.reaction.animated",
        SetupField::MaxCountCap => "bot.max_count_cap",
        SetupField::MaxSendChars => "bot.max_send_chars",
        SetupField::ActionPolicy => "bot.action_policy",
        SetupField::AuditSqlitePath => "audit.sqlite_path",
        SetupField::AuditExportMaxRows => "audit.export_max_rows",
        SetupField::AuditQueryMaxRows => "audit.query_max_rows",
        SetupField::PseudoIdHmacKeyEnv => "integrity.pseudo_id_hmac_key_env",
        SetupField::SignatureEnabled => "integrity.config_signature.enabled",
        SetupField::SignaturePath => "integrity.config_signature.detached_hmac_sha256_path",
        SetupField::SignatureHmacEnv => "integrity.config_signature.hmac_key_env",
        SetupField::LocalSelfCheck => "diagnostics.local_self_check_on_startup",
        SetupField::VerifyHardeningArtifacts => "diagnostics.verify_hardening_artifacts",
        SetupField::VerifyGeneratedArtifacts => "diagnostics.verify_generated_artifacts",
        SetupField::DependencySecurityCheckMode => "diagnostics.dependency_security_check_mode",
    }
}

fn setup_field_value(config: &StartupConfig, field: SetupField) -> String {
    match field {
        SetupField::DetectorBackend => serde_variant_name(&config.detector.backend),
        SetupField::TargetReadings => config.detector.target_readings.join(", "),
        SetupField::LiteralSequencePatterns => config.detector.literal_sequence_patterns.join(", "),
        SetupField::SpecialPhrases => config.detector.special_phrases.join(", "),
        SetupField::StampText => config.bot.stamp_text.clone(),
        SetupField::SendTemplate => config.bot.send_template.clone(),
        SetupField::ReactionEmojiId => config.bot.reaction.emoji_id.to_string(),
        SetupField::ReactionEmojiName => config.bot.reaction.emoji_name.clone(),
        SetupField::ReactionAnimated => config.bot.reaction.animated.to_string(),
        SetupField::MaxCountCap => config.bot.max_count_cap.to_string(),
        SetupField::MaxSendChars => config.bot.max_send_chars.to_string(),
        SetupField::ActionPolicy => serde_variant_name(&config.bot.action_policy),
        SetupField::AuditSqlitePath => config.audit.sqlite_path.display().to_string(),
        SetupField::AuditExportMaxRows => config.audit.export_max_rows.to_string(),
        SetupField::AuditQueryMaxRows => config.audit.query_max_rows.to_string(),
        SetupField::PseudoIdHmacKeyEnv => config.integrity.pseudo_id_hmac_key_env.clone().unwrap_or_default(),
        SetupField::SignatureEnabled => config.integrity.config_signature.is_some().to_string(),
        SetupField::SignaturePath => config
            .integrity
            .config_signature
            .as_ref()
            .map(|value| value.detached_hmac_sha256_path.display().to_string())
            .unwrap_or_else(|| "disabled".to_string()),
        SetupField::SignatureHmacEnv => config
            .integrity
            .config_signature
            .as_ref()
            .map(|value| value.hmac_key_env.clone())
            .unwrap_or_else(|| "disabled".to_string()),
        SetupField::LocalSelfCheck => config.diagnostics.local_self_check_on_startup.to_string(),
        SetupField::VerifyHardeningArtifacts => config.diagnostics.verify_hardening_artifacts.to_string(),
        SetupField::VerifyGeneratedArtifacts => config.diagnostics.verify_generated_artifacts.to_string(),
        SetupField::DependencySecurityCheckMode => serde_variant_name(&config.diagnostics.dependency_security_check_mode),
    }
}

fn setup_default_value(defaults: &StartupConfig, field: SetupField) -> String {
    setup_field_value(defaults, field)
}

fn parse_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn serde_variant_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|encoded| encoded.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn check_status_label(healthy: bool) -> &'static str {
    if healthy { "pass" } else { "warn/fail" }
}

fn format_check_status(status: &CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "pass",
        CheckStatus::Warn => "warn",
        CheckStatus::Fail => "fail",
    }
}

fn truncate_middle(value: &str, cap: usize) -> String {
    let total_chars = value.chars().count();
    if total_chars <= cap {
        return value.to_string();
    }

    let head = cap / 2;
    let tail = cap.saturating_sub(head + 3);

    let head_part: String = value.chars().take(head).collect();
    let mut tail_part: String = value.chars().rev().take(tail).collect();
    tail_part = tail_part.chars().rev().collect();

    format!("{}...{}", head_part, tail_part)
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::config::canonical_startup_config;

    fn sample_startup() -> LoadedStartupConfig {
        LoadedStartupConfig {
            app: canonical_startup_config(),
            config_path: PathBuf::from("config/oo-bot.yaml"),
            config_fingerprint: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            pseudo_id_hmac_key: None,
        }
    }

    fn render_snapshot(app: &OperatorTuiApp, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_app(frame, app))
            .expect("draw snapshot");
        let buffer = terminal.backend().buffer().clone();
        let mut lines = Vec::new();
        for y in 0..height {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buffer[(x, y)].symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    fn sample_app(screen: Screen) -> OperatorTuiApp {
        let startup = sample_startup();
        let local_self_check = LocalSelfCheckReport {
            healthy: true,
            items: vec![],
        };
        OperatorTuiApp {
            status: "snapshot".to_string(),
            startup_created: false,
            saved_config: false,
            screen,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            lsm_status: LsmStatus {
                active_modules: vec!["apparmor".to_string(), "yama".to_string()],
                apparmor_profile: Some("docker-default".to_string()),
                ..LsmStatus::default()
            },
            hardening_status: HardeningStatus {
                target: "x86_64-linux".to_string(),
                stable_release_profile: true,
                hardened_x64_requested: true,
                cet_requested: true,
                stack_protector_requested: true,
                cfi_requested: true,
                warnings: vec![],
            },
            diagnostics: DiagnosticsSummary {
                local_self_check,
                audit_db_health: "verified 10 recent rows".to_string(),
                dependency_snapshot_status: "present".to_string(),
                integrity_verify_result: "unsigned config".to_string(),
                export_safe_policy_status: "export cap=50000 query cap=10000 pseudo-id=disabled".to_string(),
                confinement_state: "AppArmor profile docker-default".to_string(),
                writable_paths: vec!["config: config".to_string(), "audit: state/audit".to_string()],
                recommendations: vec!["- prefer AppArmor".to_string(), "- keep state persistent".to_string()],
            },
            setup: SetupState {
                draft: startup.app.clone(),
                defaults: canonical_startup_config(),
                page: SetupPage::Preview,
                selected: 0,
            },
            audit: AuditState {
                rows: vec![AuditEventRow {
                    event_id: 1,
                    ts_utc: "2026-01-01T00:00:00Z".to_string(),
                    event_type: "mode_changed".to_string(),
                    schema_version: 1,
                    binary_version: "0.1.0".to_string(),
                    config_fingerprint: "fingerprint".to_string(),
                    detector_backend: "morphological_reading".to_string(),
                    matched_readings_json: "[]".to_string(),
                    sequence_hits: 0,
                    kanji_hits: 0,
                    total_count: 0,
                    special_phrase_hit: 0,
                    selected_action: "noop".to_string(),
                    suppressed_reason: String::new(),
                    mode: "normal".to_string(),
                    active_lsm: "apparmor".to_string(),
                    hardening_status: "ok".to_string(),
                    processing_time_ms: 1,
                    message_length: 0,
                    normalized_length: 0,
                    token_count: 0,
                    suspicious_flags_json: "[]".to_string(),
                    truncated_flag: 0,
                    pseudo_guild_id: String::new(),
                    pseudo_channel_id: String::new(),
                    pseudo_user_id: String::new(),
                    pseudo_message_id: String::new(),
                    prev_hash: "GENESIS".to_string(),
                    row_hash: "hash".to_string(),
                }],
                stats: AuditStats {
                    total: 1,
                    by_event_type: BTreeMap::from([(String::from("mode_changed"), 1)]),
                    by_backend: BTreeMap::from([(String::from("morphological_reading"), 1)]),
                    by_suppressed_reason: BTreeMap::new(),
                    by_mode: BTreeMap::from([(String::from("normal"), 1)]),
                },
                status: "read-only".to_string(),
                search: String::new(),
                sort: AuditSort::NewestFirst,
                mode_filter: None,
            },
            startup,
        }
    }

    #[test]
    fn dashboard_snapshot_wide() {
        let app = sample_app(Screen::Dashboard);
        let snapshot = render_snapshot(&app, 100, 24);
        assert_eq!(snapshot, include_str!("../tests/snapshots/operator_dashboard_wide.txt").trim_end());
    }

    #[test]
    fn setup_preview_snapshot_narrow() {
        let mut app = sample_app(Screen::Setup);
        app.setup.page = SetupPage::Preview;
        let snapshot = render_snapshot(&app, 72, 26);
        assert_eq!(snapshot, include_str!("../tests/snapshots/operator_setup_preview_narrow.txt").trim_end());
    }
}
