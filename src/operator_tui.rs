use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::audit::{
    AuditEventRow, AuditQueryFilter, AuditStats, AuditStore, AuditStoreConfig, SCHEMA_VERSION,
};
use crate::config::{
    canonical_startup_config, validate_startup_config, write_startup_config_to_path,
    ConfigSignatureConfig, LoadedStartupConfig, StartupConfig, CONFIG_SCHEMA_VERSION,
};
use crate::control::{
    control_socket_path, request_runtime_status, request_runtime_stop, RuntimeControlStatus,
};
use crate::security::diagnostics::{run_local_self_check, CheckStatus, LocalSelfCheckReport};
use crate::security::hardening::{detect_hardening_status, HardeningStatus};
use crate::security::lsm::{detect_lsm_status, LsmStatus};

const TUI_AUDIT_CAP: usize = 250;
const MAX_INPUT_BUFFER_LEN: usize = 4096;
const MAX_AUDIT_SEARCH_LEN: usize = 256;
const OPERATOR_TUI_I18N_YAML: &str = include_str!("../config/i18n/operator_tui.yaml");

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
    Welcome,
    Dashboard,
    Setup,
    Diagnostics,
    Audit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiLanguage {
    English,
    Japanese,
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
struct BotControlState {
    socket_path: PathBuf,
    runtime: Option<RuntimeControlStatus>,
    status: String,
    stop_armed: bool,
}

#[derive(Debug, Clone)]
struct OperatorTuiApp {
    startup: LoadedStartupConfig,
    startup_created: bool,
    saved_config: bool,
    audit_limit: usize,
    language: UiLanguage,
    screen: Screen,
    landing_screen: Screen,
    input_mode: InputMode,
    input_buffer: String,
    status: String,
    lsm_status: LsmStatus,
    hardening_status: HardeningStatus,
    diagnostics: DiagnosticsSummary,
    control: BotControlState,
    setup: SetupState,
    audit: AuditState,
}

#[derive(Debug, serde::Deserialize)]
struct UiCatalog {
    en: BTreeMap<String, String>,
    ja: BTreeMap<String, String>,
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

const INTEGRITY_FIELDS: &[SetupField] =
    &[SetupField::SignatureEnabled, SetupField::SignaturePath, SetupField::SignatureHmacEnv];

pub fn run_operator_tui(
    entry: OperatorTuiEntry,
    params: OperatorTuiParams,
) -> Result<OperatorTuiResult, String> {
    let language = detect_ui_language();
    let lsm_status = detect_lsm_status();
    let hardening_status = detect_hardening_status();
    let local_self_check = run_local_self_check(&params.startup);
    let diagnostics = build_diagnostics_summary(
        &params.startup,
        &local_self_check,
        &lsm_status,
        &hardening_status,
        language,
    );
    let control = load_control_state(&params.startup, language);
    let audit = load_audit_state(&params.startup, params.audit_limit, language);
    let landing_screen = match entry {
        OperatorTuiEntry::Dashboard => Screen::Dashboard,
        OperatorTuiEntry::Setup => Screen::Setup,
        OperatorTuiEntry::Diagnostics => Screen::Diagnostics,
        OperatorTuiEntry::Audit => Screen::Audit,
    };

    let mut app = OperatorTuiApp {
        status: default_status_message(params.startup_created, language),
        startup_created: params.startup_created,
        saved_config: false,
        audit_limit: params.audit_limit,
        language,
        screen: Screen::Welcome,
        landing_screen,
        input_mode: InputMode::Normal,
        input_buffer: String::new(),
        lsm_status,
        hardening_status,
        diagnostics,
        control,
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
            terminal.draw(|frame| render_app(frame, &app)).map_err(|err| err.to_string())?;

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

    let header = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            format!("{}   ", label(app.language, "common.app_title", "common.app_title")),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "{}={}   {}={}   {}={}   {}={}",
            label(app.language, "common.active", "common.active"),
            app.lsm_status.active_lsm_summary(),
            label(app.language, "common.config", "common.config"),
            app.startup.config_path.display(),
            label(app.language, "common.fingerprint", "common.fingerprint"),
            truncate_middle(&app.startup.config_fingerprint, 18),
            label(app.language, "common.screen", "common.screen"),
            screen_name(app.language, app.screen),
        )),
    ])])
    .block(
        Block::default()
            .title(label(app.language, "common.status_title", "common.status_title"))
            .borders(Borders::ALL)
            .border_style(overall_health_style(app)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(header, layout[0]);

    match app.screen {
        Screen::Welcome => render_welcome(frame, layout[1], app),
        Screen::Dashboard => render_dashboard(frame, layout[1], app),
        Screen::Setup => render_setup(frame, layout[1], app),
        Screen::Diagnostics => render_diagnostics(frame, layout[1], app),
        Screen::Audit => render_audit(frame, layout[1], app),
    }

    let footer_text = match app.input_mode {
        InputMode::Normal => app.status.clone(),
        InputMode::SetupEdit(field) => {
            let default = setup_default_value(app.language, &app.setup.defaults, field);
            match app.language {
                UiLanguage::English | UiLanguage::Japanese => template(
                    app.language,
                    "status.setup_edit",
                    &[("field", setup_field_label(field)), ("default", &default)],
                ),
            }
        }
        InputMode::AuditSearch => {
            label(app.language, "status.audit_search_help", "status.audit_search_help").to_string()
        }
    };
    let footer = Paragraph::new(footer_text)
        .block(
            Block::default()
                .title(label(app.language, "common.help_title", "common.help_title"))
                .borders(Borders::ALL)
                .border_style(status_style_from_text(&app.status)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, layout[2]);
}

fn render_welcome(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &OperatorTuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(8), Constraint::Length(5)])
        .split(area);

    let banner = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            label(app.language, "welcome.banner_line_1", "welcome.banner_line_1"),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            label(app.language, "welcome.banner_line_2", "welcome.banner_line_2"),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(label(app.language, "welcome.subtitle", "welcome.subtitle")),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .title(label(app.language, "welcome.title", "welcome.title"))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(banner, chunks[0]);

    let language_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(label(app.language, "welcome.language_prompt", "welcome.language_prompt")),
            language_chip(UiLanguage::English, app.language == UiLanguage::English),
            Span::raw("   "),
            language_chip(UiLanguage::Japanese, app.language == UiLanguage::Japanese),
        ]),
        Line::from(""),
        Line::from(label(app.language, "welcome.language_help", "welcome.language_help")),
        Line::from(template(
            app.language,
            "welcome.continue_prompt",
            &[("screen", screen_name(app.language, app.landing_screen))],
        )),
    ];
    let chooser = Paragraph::new(language_lines)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(label(app.language, "common.language_title", "common.language_title"))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(chooser, chunks[1]);

    let status = Paragraph::new(
        label(app.language, "welcome.quick_keys_help", "welcome.quick_keys_help").to_string(),
    )
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .title(label(app.language, "common.quick_keys_title", "common.quick_keys_title"))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(status, chunks[2]);
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
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(area)
    };

    let system = Paragraph::new(vec![
        line_kv(
            app.language,
            "common.config_schema",
            CONFIG_SCHEMA_VERSION.to_string(),
            Style::default(),
        ),
        line_kv(app.language, "common.audit_schema", SCHEMA_VERSION.to_string(), Style::default()),
        line_kv(
            app.language,
            "common.config_fingerprint",
            app.startup.config_fingerprint.clone(),
            Style::default(),
        ),
        line_kv(
            app.language,
            "common.detector_backend",
            format!("{:?}", app.startup.app.detector.backend),
            Style::default().fg(Color::Cyan),
        ),
        line_kv(
            app.language,
            "common.active_lsm",
            app.lsm_status.active_lsm_summary(),
            lsm_style(&app.lsm_status),
        ),
        line_kv(
            app.language,
            "common.confinement",
            app.diagnostics.confinement_state.clone(),
            overall_health_style(app),
        ),
        line_kv(
            app.language,
            "common.hardening",
            app.hardening_status.summary(),
            hardening_style(&app.hardening_status),
        ),
        line_kv(
            app.language,
            "common.startup_mode",
            if app.startup_created {
                label(app.language, "dashboard.fresh_bootstrap", "dashboard.fresh_bootstrap")
                    .to_string()
            } else {
                label(app.language, "dashboard.existing_config", "dashboard.existing_config")
                    .to_string()
            },
            Style::default(),
        ),
        line_kv(
            app.language,
            "common.bot_runtime",
            runtime_state_label(app.language, &app.control),
            control_state_style(&app.control),
        ),
        line_kv(
            app.language,
            "common.runtime_pid",
            runtime_pid_label(app.language, &app.control),
            control_state_style(&app.control),
        ),
    ])
    .block(
        Block::default()
            .title(label(app.language, "dashboard.title", "dashboard.title"))
            .borders(Borders::ALL)
            .border_style(overall_health_style(app)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(system, chunks[0]);

    let diagnostics = Paragraph::new(vec![
        line_kv(
            app.language,
            "common.local_self_check",
            check_status_label(app.language, app.diagnostics.local_self_check.healthy).to_string(),
            if app.diagnostics.local_self_check.healthy {
                success_style()
            } else {
                warning_style()
            },
        ),
        line_kv(
            app.language,
            "common.audit_db_health",
            app.diagnostics.audit_db_health.clone(),
            status_style_from_text(&app.diagnostics.audit_db_health),
        ),
        line_kv(
            app.language,
            "common.integrity_verify",
            app.diagnostics.integrity_verify_result.clone(),
            status_style_from_text(&app.diagnostics.integrity_verify_result),
        ),
        line_kv(
            app.language,
            "common.dependency_snapshot",
            app.diagnostics.dependency_snapshot_status.clone(),
            status_style_from_text(&app.diagnostics.dependency_snapshot_status),
        ),
        line_kv(
            app.language,
            "common.export_safe_policy",
            app.diagnostics.export_safe_policy_status.clone(),
            Style::default().fg(Color::Blue),
        ),
        line_kv(
            app.language,
            "common.startup_config",
            app.startup.config_path.display().to_string(),
            Style::default(),
        ),
        line_kv(
            app.language,
            "common.control_socket",
            truncate_middle(&app.control.socket_path.display().to_string(), 40),
            control_state_style(&app.control),
        ),
        line_kv(
            app.language,
            "common.control_status",
            app.control.status.clone(),
            status_style_from_text(&app.control.status),
        ),
    ])
    .block(
        Block::default()
            .title(label(app.language, "dashboard.health_title", "dashboard.health_title"))
            .borders(Borders::ALL)
            .border_style(overall_health_style(app)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(diagnostics, chunks[1]);

    let recommendations = Paragraph::new(format!(
        "{}:\n{}\n\n{}:\n{}",
        label(app.language, "common.writable_paths", "common.writable_paths"),
        app.diagnostics.writable_paths.join("\n"),
        label(app.language, "common.recommendations", "common.recommendations"),
        app.diagnostics.recommendations.join("\n")
    ))
    .block(
        Block::default()
            .title(label(
                app.language,
                "dashboard.runtime_env_title",
                "dashboard.runtime_env_title",
            ))
            .borders(Borders::ALL)
            .border_style(lsm_style(&app.lsm_status)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(recommendations, chunks[2]);
}

fn render_setup(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &OperatorTuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10), Constraint::Length(6)])
        .split(area);

    let page_title = Paragraph::new(format!(
        "{}={}   {}   {}   {}   {}   {}   {}   {}",
        label(app.language, "common.page", "common.page"),
        setup_page_name(app.language, app.setup.page),
        label(app.language, "setup.control.change_page", "setup.control.change_page"),
        label(app.language, "setup.control.select", "setup.control.select"),
        label(app.language, "setup.control.default", "setup.control.default"),
        label(app.language, "setup.control.edit", "setup.control.edit"),
        label(app.language, "setup.control.cycle", "setup.control.cycle"),
        label(app.language, "setup.control.preview", "setup.control.preview"),
        label(app.language, "setup.control.save", "setup.control.save"),
    ))
    .block(
        Block::default()
            .title(label(app.language, "setup.title", "setup.title"))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(page_title, chunks[0]);

    if app.setup.page == SetupPage::Preview {
        let preview = Paragraph::new(build_setup_preview(app))
            .block(
                Block::default()
                    .title(label(app.language, "setup.preview_title", "setup.preview_title"))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(preview, chunks[1]);
    } else if app.setup.page == SetupPage::Runtime {
        let runtime = Paragraph::new(build_runtime_environment_block(app))
            .block(
                Block::default()
                    .title(label(app.language, "setup.runtime_title", "setup.runtime_title"))
                    .borders(Borders::ALL)
                    .border_style(lsm_style(&app.lsm_status)),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(runtime, chunks[1]);
    } else {
        let body = Paragraph::new(build_setup_field_block(app))
            .block(
                Block::default()
                    .title(label(app.language, "setup.fields_title", "setup.fields_title"))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(body, chunks[1]);
    }

    let validation = Paragraph::new(build_setup_validation_lines(app))
        .block(
            Block::default()
                .title(label(app.language, "setup.validation_title", "setup.validation_title"))
                .borders(Borders::ALL)
                .border_style(validation_border_style(app)),
        )
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

    let summary = Paragraph::new(vec![
        line_kv(
            app.language,
            "common.active_lsm",
            app.lsm_status.active_lsm_summary(),
            lsm_style(&app.lsm_status),
        ),
        line_kv(
            app.language,
            "common.current_confinement",
            app.diagnostics.confinement_state.clone(),
            overall_health_style(app),
        ),
        line_kv(
            app.language,
            "common.hardening_status",
            app.hardening_status.summary(),
            hardening_style(&app.hardening_status),
        ),
        line_kv(
            app.language,
            "common.config_fingerprint",
            app.startup.config_fingerprint.clone(),
            Style::default(),
        ),
        line_kv(
            app.language,
            "common.detector_backend",
            format!("{:?}", app.startup.app.detector.backend),
            Style::default().fg(Color::Cyan),
        ),
        line_kv(
            app.language,
            "common.audit_db_health",
            app.diagnostics.audit_db_health.clone(),
            status_style_from_text(&app.diagnostics.audit_db_health),
        ),
        line_kv(
            app.language,
            "common.schema_version",
            CONFIG_SCHEMA_VERSION.to_string(),
            Style::default(),
        ),
        line_kv(
            app.language,
            "common.integrity_verify_result",
            app.diagnostics.integrity_verify_result.clone(),
            status_style_from_text(&app.diagnostics.integrity_verify_result),
        ),
        line_kv(
            app.language,
            "common.export_safe_policy",
            app.diagnostics.export_safe_policy_status.clone(),
            Style::default().fg(Color::Blue),
        ),
        line_kv(
            app.language,
            "common.dependency_audit_snapshot",
            app.diagnostics.dependency_snapshot_status.clone(),
            status_style_from_text(&app.diagnostics.dependency_snapshot_status),
        ),
    ])
    .block(
        Block::default()
            .title(label(app.language, "diagnostics.summary_title", "diagnostics.summary_title"))
            .borders(Borders::ALL)
            .border_style(overall_health_style(app)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, chunks[0]);

    let items = app
        .diagnostics
        .local_self_check
        .items
        .iter()
        .map(|item| {
            Line::from(vec![
                Span::styled(
                    format!("[{}] ", format_check_status(app.language, &item.status)),
                    check_status_style(&item.status),
                ),
                Span::styled(
                    format!("{}: ", translate_check_name(app.language, &item.name)),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(item.detail.clone()),
            ])
        })
        .collect::<Vec<_>>();
    let details = Paragraph::new(items)
        .block(
            Block::default()
                .title(label(app.language, "diagnostics.detail_title", "diagnostics.detail_title"))
                .borders(Borders::ALL)
                .border_style(overall_health_style(app)),
        )
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
        "{}: {}   {}: {}   {}: {:?}   {}: {:?}   {}: {:?}\n{}: {}\n{}: {}\n{}: {}",
        label(app.language, "common.rows_loaded", "common.rows_loaded"),
        app.audit.rows.len(),
        label(app.language, "common.rows_shown", "common.rows_shown"),
        filtered.len(),
        label(app.language, "common.search", "common.search"),
        if app.audit.search.is_empty() { None } else { Some(app.audit.search.as_str()) },
        label(app.language, "common.sort", "common.sort"),
        app.audit.sort,
        label(app.language, "common.mode_filter", "common.mode_filter"),
        app.audit.mode_filter.as_deref(),
        label(app.language, "common.backend_comparison", "common.backend_comparison"),
        format_counts(app.language, &app.audit.stats.by_backend),
        label(app.language, "common.suppression_reasons", "common.suppression_reasons"),
        format_counts(app.language, &app.audit.stats.by_suppressed_reason),
        label(app.language, "common.mode_transitions", "common.mode_transitions"),
        filtered.iter().filter(|row| row.event_type == "mode_changed").count(),
    ))
    .block(
        Block::default()
            .title(label(app.language, "audit.browser_title", "audit.browser_title"))
            .borders(Borders::ALL)
            .border_style(status_style_from_text(&app.audit.status)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, chunks[0]);

    let status = Paragraph::new(vec![
        Line::from(Span::styled(
            app.audit.status.clone(),
            status_style_from_text(&app.audit.status),
        )),
        Line::from(match app.language {
            UiLanguage::English | UiLanguage::Japanese => {
                label(app.language, "audit.controls_help", "audit.controls_help").to_string()
            }
        }),
    ])
    .block(
        Block::default()
            .title(label(app.language, "audit.controls_title", "audit.controls_title"))
            .borders(Borders::ALL)
            .border_style(status_style_from_text(&app.audit.status)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(status, chunks[1]);

    let lines = filtered
        .iter()
        .take(TUI_AUDIT_CAP)
        .map(|row| {
            let style = audit_row_style(row);
            Line::from(Span::styled(
                format!(
                    "#{} {} {}={} {}={} {}={} {}={}",
                    row.event_id,
                    row.event_type,
                    label(app.language, "common.backend", "common.backend"),
                    row.detector_backend,
                    label(app.language, "common.action", "common.action"),
                    row.selected_action,
                    label(app.language, "common.reason", "common.reason"),
                    if row.suppressed_reason.is_empty() {
                        "-"
                    } else {
                        row.suppressed_reason.as_str()
                    },
                    label(app.language, "common.mode", "common.mode"),
                    row.mode,
                ),
                style,
            ))
        })
        .collect::<Vec<_>>();

    let rows = Paragraph::new(if lines.is_empty() {
        vec![Line::from(label(app.language, "audit.no_rows", "audit.no_rows"))]
    } else {
        lines
    })
    .block(
        Block::default()
            .title(label(app.language, "audit.rows_title", "audit.rows_title"))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(rows, chunks[2]);
}

fn handle_global_keys(app: &mut OperatorTuiApp, code: KeyCode) -> Result<bool, String> {
    if app.screen == Screen::Welcome {
        return handle_welcome_keys(app, code);
    }

    match code {
        KeyCode::Char('x') => {
            if app.control.stop_armed {
                match request_runtime_stop(&app.startup.config_path, "tui") {
                    Ok(message) => {
                        app.control.stop_armed = false;
                        refresh_control_state(app);
                        app.status = template(
                            app.language,
                            "status.stop_requested",
                            &[("message", &message)],
                        );
                    }
                    Err(err) => {
                        app.control.stop_armed = false;
                        refresh_control_state(app);
                        app.status =
                            template(app.language, "status.stop_failed", &[("error", &err)]);
                    }
                }
            } else {
                app.control.stop_armed = true;
                app.status =
                    label(app.language, "status.stop_confirm", "status.stop_confirm").to_string();
            }
            return Ok(false);
        }
        KeyCode::Esc if app.control.stop_armed => {
            app.control.stop_armed = false;
            app.status =
                label(app.language, "status.stop_cancelled", "status.stop_cancelled").to_string();
            return Ok(false);
        }
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('l') => {
            toggle_language(app);
            app.status = language_switched_message(app.language);
        }
        KeyCode::Char('R') => {
            refresh_runtime_snapshot(app);
            app.status =
                label(app.language, "status.runtime_refreshed", "status.runtime_refreshed")
                    .to_string();
        }
        KeyCode::Char('1') => app.screen = Screen::Dashboard,
        KeyCode::Char('2') => app.screen = Screen::Setup,
        KeyCode::Char('3') => app.screen = Screen::Diagnostics,
        KeyCode::Char('4') => app.screen = Screen::Audit,
        _ => {}
    }

    if app.control.stop_armed {
        app.control.stop_armed = false;
    }

    match app.screen {
        Screen::Setup => handle_setup_normal_keys(app, code)?,
        Screen::Audit => handle_audit_normal_keys(app, code)?,
        Screen::Welcome | Screen::Dashboard | Screen::Diagnostics => {}
    }

    Ok(false)
}

fn handle_welcome_keys(app: &mut OperatorTuiApp, code: KeyCode) -> Result<bool, String> {
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('e') | KeyCode::Left => {
            app.language = UiLanguage::English;
            relocalize_app(app);
            app.status = language_switched_message(app.language);
        }
        KeyCode::Char('j') | KeyCode::Right => {
            app.language = UiLanguage::Japanese;
            relocalize_app(app);
            app.status = language_switched_message(app.language);
        }
        KeyCode::Char('l') => {
            toggle_language(app);
            app.status = language_switched_message(app.language);
        }
        KeyCode::Enter => {
            app.screen = app.landing_screen;
            app.status = default_status_message(app.startup_created, app.language);
        }
        _ => {}
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
        KeyCode::Up if app.setup.selected > 0 => {
            app.setup.selected -= 1;
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
                app.status = template(
                    app.language,
                    "status.restore_default",
                    &[("field", setup_field_label(field))],
                );
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
                app.status =
                    label(app.language, "status.move_preview", "status.move_preview").to_string();
            } else {
                validate_startup_config(&app.setup.draft).map_err(|err| err.to_string())?;
                write_startup_config_to_path(&app.startup.config_path, &app.setup.draft)
                    .map_err(|err| err.to_string())?;
                app.startup =
                    crate::config::load_startup_config_from_path(&app.startup.config_path)
                        .map_err(|err| err.to_string())?;
                let local_self_check = run_local_self_check(&app.startup);
                app.diagnostics = build_diagnostics_summary(
                    &app.startup,
                    &local_self_check,
                    &app.lsm_status,
                    &app.hardening_status,
                    app.language,
                );
                app.control = load_control_state(&app.startup, app.language);
                app.audit = load_audit_state(&app.startup, app.audit_limit, app.language);
                app.saved_config = true;
                app.status = template(
                    app.language,
                    "status.saved",
                    &[("path", &app.startup.config_path.display().to_string())],
                );
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_setup_edit_keys(
    app: &mut OperatorTuiApp,
    field: SetupField,
    code: KeyCode,
) -> Result<bool, String> {
    match code {
        KeyCode::Esc => return Ok(true),
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Enter => {
            apply_custom_field_input(&mut app.setup, field, &app.input_buffer, app.language)?;
            app.status =
                template(app.language, "status.updated", &[("field", setup_field_label(field))]);
            app.input_buffer.clear();
            return Ok(true);
        }
        KeyCode::Char(ch) if app.input_buffer.len() < MAX_INPUT_BUFFER_LEN => {
            app.input_buffer.push(ch);
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
            app.audit = load_audit_state(&app.startup, app.audit_limit, app.language);
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
            app.audit.search = app.input_buffer.trim().chars().take(MAX_AUDIT_SEARCH_LEN).collect();
            app.status = template(
                app.language,
                "status.audit_search_updated",
                &[("search", &format!("{:?}", app.audit.search))],
            );
            return Ok(true);
        }
        KeyCode::Char(ch) if app.input_buffer.len() < MAX_INPUT_BUFFER_LEN => {
            app.input_buffer.push(ch);
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
    language: UiLanguage,
) -> DiagnosticsSummary {
    let audit_db_health = local_self_check
        .items
        .iter()
        .find(|item| item.name == "audit_db_health")
        .map(|item| item.detail.clone())
        .unwrap_or_else(|| {
            label(language, "diagnostics.not_checked", "diagnostics.not_checked").to_string()
        });

    DiagnosticsSummary {
        local_self_check: local_self_check.clone(),
        audit_db_health,
        dependency_snapshot_status: dependency_snapshot_status(
            &startup.app.diagnostics.security_snapshot_path,
            language,
        ),
        integrity_verify_result: if startup.app.integrity.config_signature.is_some() {
            label(language, "diagnostics.signature_verified", "diagnostics.signature_verified")
                .to_string()
        } else {
            label(language, "diagnostics.unsigned_config", "diagnostics.unsigned_config")
                .to_string()
        },
        export_safe_policy_status: format!(
            "{}={} {}={} {}={}",
            label(language, "common.export_cap", "common.export_cap"),
            startup.app.audit.export_max_rows,
            label(language, "common.query_cap", "common.query_cap"),
            startup.app.audit.query_max_rows,
            label(language, "common.pseudo_id", "common.pseudo_id"),
            if startup.pseudo_id_hmac_key.is_some() {
                label(language, "common.enabled", "common.enabled")
            } else {
                label(language, "common.disabled", "common.disabled")
            }
        ),
        confinement_state: confinement_state(lsm_status, hardening_status, language),
        writable_paths: vec![
            writable_parent_label(language, "common.config", &startup.config_path),
            writable_parent_label(language, "screen.audit", &startup.app.audit.sqlite_path),
            writable_parent_label(
                language,
                "common.security_snapshot",
                &startup.app.diagnostics.security_snapshot_path,
            ),
        ],
        recommendations: runtime_recommendations(lsm_status, hardening_status, language),
    }
}

fn load_audit_state(
    startup: &LoadedStartupConfig,
    audit_limit: usize,
    language: UiLanguage,
) -> AuditState {
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
            status: label(language, "audit.status.missing_db", "audit.status.missing_db")
                .to_string(),
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
                status: template(language, "audit.status.open_failed", &[("error", &err)]),
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
        status: label(language, "audit.status.ready", "audit.status.ready").to_string(),
        search: String::new(),
        sort: AuditSort::NewestFirst,
        mode_filter: None,
    }
}

fn load_control_state(startup: &LoadedStartupConfig, language: UiLanguage) -> BotControlState {
    let socket_path = control_socket_path(&startup.config_path);
    match request_runtime_status(&startup.config_path) {
        Ok(runtime) => BotControlState {
            socket_path,
            runtime: Some(runtime),
            status: label(language, "control.status.connected", "control.status.connected")
                .to_string(),
            stop_armed: false,
        },
        Err(err) => BotControlState {
            socket_path,
            runtime: None,
            status: template(language, "control.status.unavailable", &[("error", &err)]),
            stop_armed: false,
        },
    }
}

fn build_setup_field_block(app: &OperatorTuiApp) -> String {
    let mut out = vec![format!("{}\n", setup_page_help(app.language, app.setup.page))];

    for (index, field) in setup_fields_for_page(app.setup.page).iter().enumerate() {
        let marker = if index == app.setup.selected { ">" } else { " " };
        out.push(format!(
            "{marker} {} = {}\n  {}: {}\n",
            setup_field_label(*field),
            setup_field_value(app.language, &app.setup.draft, *field),
            label(app.language, "common.default", "common.default"),
            setup_default_value(app.language, &app.setup.defaults, *field)
        ));
    }

    out.join("\n")
}

fn build_runtime_environment_block(app: &OperatorTuiApp) -> String {
    format!(
        "{}: {}\n{}: {}\n{}:\n{}\n\n{}:\n{}",
        label(app.language, "common.detected_lsm", "common.detected_lsm"),
        app.lsm_status.active_lsm_summary(),
        label(app.language, "common.hardening_status", "common.hardening_status"),
        app.hardening_status.summary(),
        label(app.language, "common.writable_paths", "common.writable_paths"),
        app.diagnostics.writable_paths.join("\n"),
        label(
            app.language,
            "common.service_container_recommendations",
            "common.service_container_recommendations"
        ),
        app.diagnostics.recommendations.join("\n"),
    )
}

fn build_setup_preview(app: &OperatorTuiApp) -> String {
    let yaml = crate::config::render_startup_config_yaml(&app.setup.draft).unwrap_or_else(|err| {
        format!(
            "{}\n",
            template(app.language, "setup.preview.render_failed", &[("error", &err.to_string())],)
        )
    });
    format!("{}:\n\n{yaml}", label(app.language, "setup.preview.label", "setup.preview.label"))
}

fn build_setup_validation_lines(app: &OperatorTuiApp) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    match validate_startup_config(&app.setup.draft) {
        Ok(()) => lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", format_check_status(app.language, &CheckStatus::Pass)),
                success_style(),
            ),
            Span::raw(label(app.language, "validation.pass", "validation.pass").to_string()),
        ])),
        Err(err) => lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", format_check_status(app.language, &CheckStatus::Fail)),
                error_style(),
            ),
            Span::raw(template(app.language, "validation.fail", &[("error", &err.to_string())])),
        ])),
    }

    if let Some(signature) = &app.setup.draft.integrity.config_signature {
        lines.push(Line::from(vec![
            Span::styled("[info] ", info_style()),
            Span::raw(template(
                app.language,
                "validation.signature_info",
                &[
                    ("path", &signature.detached_hmac_sha256_path.display().to_string()),
                    ("env", &signature.hmac_key_env),
                    ("present", &std::env::var(&signature.hmac_key_env).is_ok().to_string()),
                ],
            )),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("[info] ", info_style()),
            Span::raw(
                label(
                    app.language,
                    "validation.signature_disabled",
                    "validation.signature_disabled",
                )
                .to_string(),
            ),
        ]));
    }

    lines
}

fn dependency_snapshot_status(path: &Path, language: UiLanguage) -> String {
    match fs::metadata(path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|value| value.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|| label(language, "common.unknown", "common.unknown").to_string());
            template(
                language,
                "diagnostics.snapshot_present",
                &[("path", &path.display().to_string()), ("mtime", &modified)],
            )
        }
        Err(_) => template(
            language,
            "diagnostics.snapshot_missing",
            &[("path", &path.display().to_string())],
        ),
    }
}

fn confinement_state(
    lsm_status: &LsmStatus,
    hardening_status: &HardeningStatus,
    language: UiLanguage,
) -> String {
    if let Some(profile) = &lsm_status.apparmor_profile {
        return template(language, "diagnostics.apparmor_profile", &[("profile", profile)]);
    }
    if let Some(context) = &lsm_status.selinux_context {
        return template(language, "diagnostics.selinux_context", &[("context", context)]);
    }
    if lsm_status.active_modules.is_empty() {
        return template(
            language,
            "diagnostics.unconfined",
            &[("summary", &hardening_status.summary())],
        );
    }
    template(
        language,
        "diagnostics.lsm_summary",
        &[("lsm", &lsm_status.active_lsm_summary()), ("summary", &hardening_status.summary())],
    )
}

fn writable_parent_label(language: UiLanguage, label_key: &str, path: &Path) -> String {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    template(
        language,
        "diagnostics.persisted_label",
        &[
            ("label", label(language, label_key, label_key)),
            ("path", &parent.display().to_string()),
        ],
    )
}

fn runtime_recommendations(
    lsm_status: &LsmStatus,
    hardening_status: &HardeningStatus,
    language: UiLanguage,
) -> Vec<String> {
    let mut out = Vec::new();
    if lsm_status.major_lsm.is_none() {
        out.push(
            label(language, "recommendation.prefer_lsm", "recommendation.prefer_lsm").to_string(),
        );
    }
    if !hardening_status.hardened_x64_requested {
        out.push(
            label(language, "recommendation.hardened_release", "recommendation.hardened_release")
                .to_string(),
        );
    }
    out.push(
        label(language, "recommendation.mount_state", "recommendation.mount_state").to_string(),
    );
    out.push(
        label(language, "recommendation.persist_config", "recommendation.persist_config")
            .to_string(),
    );
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
                    row.event_type,
                    row.detector_backend,
                    row.selected_action,
                    row.suppressed_reason,
                    row.mode
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
        AuditSort::NewestFirst => rows.sort_by_key(|row| std::cmp::Reverse(row.event_id)),
        AuditSort::OldestFirst => rows.sort_by_key(|row| row.event_id),
        AuditSort::EventType => {
            rows.sort_by(|a, b| a.event_type.cmp(&b.event_type).then(a.event_id.cmp(&b.event_id)))
        }
        AuditSort::Mode => {
            rows.sort_by(|a, b| a.mode.cmp(&b.mode).then(a.event_id.cmp(&b.event_id)))
        }
    }

    rows
}

fn format_counts(language: UiLanguage, counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return label(language, "common.none", "common.none").to_string();
    }
    counts.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join(", ")
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

fn setup_page_name(language: UiLanguage, page: SetupPage) -> &'static str {
    match page {
        SetupPage::Detector => label(language, "setup.page.detector", "setup.page.detector"),
        SetupPage::Bot => label(language, "setup.page.bot", "setup.page.bot"),
        SetupPage::Audit => label(language, "setup.page.audit", "setup.page.audit"),
        SetupPage::Diagnostics => {
            label(language, "setup.page.diagnostics", "setup.page.diagnostics")
        }
        SetupPage::Integrity => label(language, "setup.page.integrity", "setup.page.integrity"),
        SetupPage::Runtime => label(language, "setup.page.runtime", "setup.page.runtime"),
        SetupPage::Preview => label(language, "setup.page.preview", "setup.page.preview"),
    }
}

fn setup_page_help(language: UiLanguage, page: SetupPage) -> &'static str {
    match page {
        SetupPage::Detector => label(language, "setup.help.detector", "setup.help.detector"),
        SetupPage::Bot => label(language, "setup.help.bot", "setup.help.bot"),
        SetupPage::Audit => label(language, "setup.help.audit", "setup.help.audit"),
        SetupPage::Diagnostics => {
            label(language, "setup.help.diagnostics", "setup.help.diagnostics")
        }
        SetupPage::Integrity => label(language, "setup.help.integrity", "setup.help.integrity"),
        SetupPage::Runtime => label(language, "setup.help.runtime", "setup.help.runtime"),
        SetupPage::Preview => label(language, "setup.help.preview", "setup.help.preview"),
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
        SetupField::DetectorBackend => {
            setup.draft.detector.backend = setup.defaults.detector.backend
        }
        SetupField::TargetReadings => {
            setup.draft.detector.target_readings = setup.defaults.detector.target_readings.clone()
        }
        SetupField::LiteralSequencePatterns => {
            setup.draft.detector.literal_sequence_patterns =
                setup.defaults.detector.literal_sequence_patterns.clone()
        }
        SetupField::SpecialPhrases => {
            setup.draft.detector.special_phrases = setup.defaults.detector.special_phrases.clone()
        }
        SetupField::StampText => setup.draft.bot.stamp_text = setup.defaults.bot.stamp_text.clone(),
        SetupField::SendTemplate => {
            setup.draft.bot.send_template = setup.defaults.bot.send_template.clone()
        }
        SetupField::ReactionEmojiId => {
            setup.draft.bot.reaction.emoji_id = setup.defaults.bot.reaction.emoji_id
        }
        SetupField::ReactionEmojiName => {
            setup.draft.bot.reaction.emoji_name = setup.defaults.bot.reaction.emoji_name.clone()
        }
        SetupField::ReactionAnimated => {
            setup.draft.bot.reaction.animated = setup.defaults.bot.reaction.animated
        }
        SetupField::MaxCountCap => setup.draft.bot.max_count_cap = setup.defaults.bot.max_count_cap,
        SetupField::MaxSendChars => {
            setup.draft.bot.max_send_chars = setup.defaults.bot.max_send_chars
        }
        SetupField::ActionPolicy => {
            setup.draft.bot.action_policy = setup.defaults.bot.action_policy.clone()
        }
        SetupField::AuditSqlitePath => {
            setup.draft.audit.sqlite_path = setup.defaults.audit.sqlite_path.clone()
        }
        SetupField::AuditExportMaxRows => {
            setup.draft.audit.export_max_rows = setup.defaults.audit.export_max_rows
        }
        SetupField::AuditQueryMaxRows => {
            setup.draft.audit.query_max_rows = setup.defaults.audit.query_max_rows
        }
        SetupField::PseudoIdHmacKeyEnv => {
            setup.draft.integrity.pseudo_id_hmac_key_env =
                setup.defaults.integrity.pseudo_id_hmac_key_env.clone()
        }
        SetupField::SignatureEnabled => {
            setup.draft.integrity.config_signature =
                setup.defaults.integrity.config_signature.clone()
        }
        SetupField::SignaturePath => {
            setup.draft.integrity.config_signature =
                setup.defaults.integrity.config_signature.clone();
        }
        SetupField::SignatureHmacEnv => {
            setup.draft.integrity.config_signature =
                setup.defaults.integrity.config_signature.clone();
        }
        SetupField::LocalSelfCheck => {
            setup.draft.diagnostics.local_self_check_on_startup =
                setup.defaults.diagnostics.local_self_check_on_startup
        }
        SetupField::VerifyHardeningArtifacts => {
            setup.draft.diagnostics.verify_hardening_artifacts =
                setup.defaults.diagnostics.verify_hardening_artifacts;
        }
        SetupField::VerifyGeneratedArtifacts => {
            setup.draft.diagnostics.verify_generated_artifacts =
                setup.defaults.diagnostics.verify_generated_artifacts;
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
                crate::app::analyze_message::ActionPolicy::ReactOrSend => {
                    crate::app::analyze_message::ActionPolicy::ReactOnly
                }
                crate::app::analyze_message::ActionPolicy::ReactOnly => {
                    crate::app::analyze_message::ActionPolicy::NoOutbound
                }
                crate::app::analyze_message::ActionPolicy::NoOutbound => {
                    crate::app::analyze_message::ActionPolicy::ReactOrSend
                }
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
                    crate::config::DependencySecurityCheckMode::Disabled => {
                        crate::config::DependencySecurityCheckMode::OfflineSnapshot
                    }
                    crate::config::DependencySecurityCheckMode::OfflineSnapshot => {
                        crate::config::DependencySecurityCheckMode::Disabled
                    }
                };
        }
        SetupField::SignatureEnabled => {
            setup.draft.integrity.config_signature =
                if setup.draft.integrity.config_signature.is_some() {
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

fn apply_custom_field_input(
    setup: &mut SetupState,
    field: SetupField,
    raw: &str,
    language: UiLanguage,
) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        restore_default_for_field(setup, field);
        return Ok(());
    }

    match field {
        SetupField::DetectorBackend => {
            setup.draft.detector.backend =
                serde_yaml::from_str(trimmed).map_err(|err| err.to_string())?;
        }
        SetupField::TargetReadings => setup.draft.detector.target_readings = parse_csv(trimmed),
        SetupField::LiteralSequencePatterns => {
            setup.draft.detector.literal_sequence_patterns = parse_csv(trimmed)
        }
        SetupField::SpecialPhrases => setup.draft.detector.special_phrases = parse_csv(trimmed),
        SetupField::StampText => setup.draft.bot.stamp_text = trimmed.to_string(),
        SetupField::SendTemplate => setup.draft.bot.send_template = trimmed.to_string(),
        SetupField::ReactionEmojiId => {
            setup.draft.bot.reaction.emoji_id = trimmed.parse::<u64>().map_err(|_| {
                label(language, "error.emoji_id_u64", "error.emoji_id_u64").to_string()
            })?;
        }
        SetupField::ReactionEmojiName => setup.draft.bot.reaction.emoji_name = trimmed.to_string(),
        SetupField::ReactionAnimated => {}
        SetupField::MaxCountCap => {
            setup.draft.bot.max_count_cap = trimmed.parse::<usize>().map_err(|_| {
                label(language, "error.max_count_cap_usize", "error.max_count_cap_usize")
                    .to_string()
            })?;
        }
        SetupField::MaxSendChars => {
            setup.draft.bot.max_send_chars = trimmed.parse::<usize>().map_err(|_| {
                label(language, "error.max_send_chars_usize", "error.max_send_chars_usize")
                    .to_string()
            })?;
        }
        SetupField::ActionPolicy => {}
        SetupField::AuditSqlitePath => setup.draft.audit.sqlite_path = PathBuf::from(trimmed),
        SetupField::AuditExportMaxRows => {
            setup.draft.audit.export_max_rows = trimmed.parse::<usize>().map_err(|_| {
                label(language, "error.export_max_rows_usize", "error.export_max_rows_usize")
                    .to_string()
            })?;
        }
        SetupField::AuditQueryMaxRows => {
            setup.draft.audit.query_max_rows = trimmed.parse::<usize>().map_err(|_| {
                label(language, "error.query_max_rows_usize", "error.query_max_rows_usize")
                    .to_string()
            })?;
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

fn setup_field_value(language: UiLanguage, config: &StartupConfig, field: SetupField) -> String {
    match field {
        SetupField::DetectorBackend => serde_variant_name(language, &config.detector.backend),
        SetupField::TargetReadings => format!(
            "{} ({})",
            config.detector.target_readings.join(", "),
            label(language, "setup.target_readings_hint", "setup.target_readings_hint")
        ),
        SetupField::LiteralSequencePatterns => config.detector.literal_sequence_patterns.join(", "),
        SetupField::SpecialPhrases => config.detector.special_phrases.join(", "),
        SetupField::StampText => config.bot.stamp_text.clone(),
        SetupField::SendTemplate => config.bot.send_template.clone(),
        SetupField::ReactionEmojiId => config.bot.reaction.emoji_id.to_string(),
        SetupField::ReactionEmojiName => config.bot.reaction.emoji_name.clone(),
        SetupField::ReactionAnimated => config.bot.reaction.animated.to_string(),
        SetupField::MaxCountCap => config.bot.max_count_cap.to_string(),
        SetupField::MaxSendChars => config.bot.max_send_chars.to_string(),
        SetupField::ActionPolicy => serde_variant_name(language, &config.bot.action_policy),
        SetupField::AuditSqlitePath => config.audit.sqlite_path.display().to_string(),
        SetupField::AuditExportMaxRows => config.audit.export_max_rows.to_string(),
        SetupField::AuditQueryMaxRows => config.audit.query_max_rows.to_string(),
        SetupField::PseudoIdHmacKeyEnv => {
            config.integrity.pseudo_id_hmac_key_env.clone().unwrap_or_default()
        }
        SetupField::SignatureEnabled => config.integrity.config_signature.is_some().to_string(),
        SetupField::SignaturePath => config
            .integrity
            .config_signature
            .as_ref()
            .map(|value| value.detached_hmac_sha256_path.display().to_string())
            .unwrap_or_else(|| label(language, "common.disabled", "common.disabled").to_string()),
        SetupField::SignatureHmacEnv => config
            .integrity
            .config_signature
            .as_ref()
            .map(|value| value.hmac_key_env.clone())
            .unwrap_or_else(|| label(language, "common.disabled", "common.disabled").to_string()),
        SetupField::LocalSelfCheck => config.diagnostics.local_self_check_on_startup.to_string(),
        SetupField::VerifyHardeningArtifacts => {
            config.diagnostics.verify_hardening_artifacts.to_string()
        }
        SetupField::VerifyGeneratedArtifacts => {
            config.diagnostics.verify_generated_artifacts.to_string()
        }
        SetupField::DependencySecurityCheckMode => {
            serde_variant_name(language, &config.diagnostics.dependency_security_check_mode)
        }
    }
}

fn setup_default_value(
    language: UiLanguage,
    defaults: &StartupConfig,
    field: SetupField,
) -> String {
    setup_field_value(language, defaults, field)
}

fn parse_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn serde_variant_name<T: serde::Serialize>(language: UiLanguage, value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|encoded| encoded.as_str().map(ToString::to_string))
        .unwrap_or_else(|| {
            label(language, "common.unknown_variant", "common.unknown_variant").to_string()
        })
}

fn check_status_label(language: UiLanguage, healthy: bool) -> &'static str {
    if healthy {
        label(language, "common.pass", "common.pass")
    } else {
        label(language, "common.warn_fail", "common.warn_fail")
    }
}

fn format_check_status(language: UiLanguage, status: &CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => label(language, "common.pass", "common.pass"),
        CheckStatus::Warn => label(language, "common.warn", "common.warn"),
        CheckStatus::Fail => label(language, "common.fail", "common.fail"),
    }
}

fn catalog() -> &'static UiCatalog {
    static CATALOG: OnceLock<UiCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_yaml::from_str(OPERATOR_TUI_I18N_YAML).expect("operator tui i18n catalog")
    })
}

fn label<'a>(language: UiLanguage, key: &'a str, fallback: &'a str) -> &'a str {
    let map = match language {
        UiLanguage::English => &catalog().en,
        UiLanguage::Japanese => &catalog().ja,
    };
    map.get(key).map(String::as_str).unwrap_or_else(|| match language {
        UiLanguage::English => key,
        UiLanguage::Japanese => fallback,
    })
}

fn template(language: UiLanguage, key: &str, replacements: &[(&str, &str)]) -> String {
    let mut value = label(language, key, key).to_string();
    for (name, replacement) in replacements {
        value = value.replace(&format!("{{{name}}}"), replacement);
    }
    value
}

fn detect_ui_language() -> UiLanguage {
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if locale.starts_with("ja") {
        UiLanguage::Japanese
    } else {
        UiLanguage::English
    }
}

fn toggle_language(app: &mut OperatorTuiApp) {
    app.language = match app.language {
        UiLanguage::English => UiLanguage::Japanese,
        UiLanguage::Japanese => UiLanguage::English,
    };
    relocalize_app(app);
}

fn relocalize_app(app: &mut OperatorTuiApp) {
    app.diagnostics = build_diagnostics_summary(
        &app.startup,
        &app.diagnostics.local_self_check,
        &app.lsm_status,
        &app.hardening_status,
        app.language,
    );
    let stop_armed = app.control.stop_armed;
    app.control = load_control_state(&app.startup, app.language);
    app.control.stop_armed = stop_armed;
    app.audit = load_audit_state(&app.startup, app.audit_limit, app.language);
}

fn refresh_control_state(app: &mut OperatorTuiApp) {
    let stop_armed = app.control.stop_armed;
    app.control = load_control_state(&app.startup, app.language);
    app.control.stop_armed = stop_armed;
}

fn refresh_runtime_snapshot(app: &mut OperatorTuiApp) {
    app.lsm_status = detect_lsm_status();
    app.hardening_status = detect_hardening_status();
    let local_self_check = run_local_self_check(&app.startup);
    app.diagnostics = build_diagnostics_summary(
        &app.startup,
        &local_self_check,
        &app.lsm_status,
        &app.hardening_status,
        app.language,
    );
    refresh_control_state(app);
    app.audit = load_audit_state(&app.startup, app.audit_limit, app.language);
}

fn default_status_message(startup_created: bool, language: UiLanguage) -> String {
    if startup_created {
        label(language, "status.startup_created", "status.startup_created").to_string()
    } else {
        label(language, "status.default_nav", "status.default_nav").to_string()
    }
}

fn language_switched_message(language: UiLanguage) -> String {
    match language {
        UiLanguage::English => {
            label(language, "status.language_english", "status.language_english").to_string()
        }
        UiLanguage::Japanese => {
            label(language, "status.language_japanese", "status.language_japanese").to_string()
        }
    }
}

fn screen_name(language: UiLanguage, screen: Screen) -> &'static str {
    match screen {
        Screen::Welcome => label(language, "screen.welcome", "screen.welcome"),
        Screen::Dashboard => label(language, "screen.dashboard", "screen.dashboard"),
        Screen::Setup => label(language, "screen.setup", "screen.setup"),
        Screen::Diagnostics => label(language, "screen.diagnostics", "screen.diagnostics"),
        Screen::Audit => label(language, "screen.audit", "screen.audit"),
    }
}

fn language_chip(language: UiLanguage, selected: bool) -> Span<'static> {
    let base = match language {
        UiLanguage::English => label(language, "common.english", "English"),
        UiLanguage::Japanese => label(language, "common.japanese", "日本語"),
    };
    let text = if selected { format!("[{base}]") } else { base.to_string() };
    let style = if selected {
        Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    Span::styled(text, style)
}

fn success_style() -> Style {
    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
}

fn warning_style() -> Style {
    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

fn error_style() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

fn info_style() -> Style {
    Style::default().fg(Color::Cyan)
}

fn check_status_style(status: &CheckStatus) -> Style {
    match status {
        CheckStatus::Pass => success_style(),
        CheckStatus::Warn => warning_style(),
        CheckStatus::Fail => error_style(),
    }
}

fn status_style_from_text(text: &str) -> Style {
    let lower = text.to_ascii_lowercase();
    let has_error_keyword =
        label(UiLanguage::English, "style.error_keywords", "style.error_keywords")
            .split(',')
            .any(|keyword| !keyword.is_empty() && lower.contains(keyword))
            || label(UiLanguage::Japanese, "style.error_keywords", "style.error_keywords")
                .split(',')
                .any(|keyword| !keyword.is_empty() && text.contains(keyword));
    if has_error_keyword {
        error_style()
    } else {
        let has_warn_keyword =
            label(UiLanguage::English, "style.warn_keywords", "style.warn_keywords")
                .split(',')
                .any(|keyword| !keyword.is_empty() && lower.contains(keyword))
                || label(UiLanguage::Japanese, "style.warn_keywords", "style.warn_keywords")
                    .split(',')
                    .any(|keyword| !keyword.is_empty() && text.contains(keyword));
        if has_warn_keyword {
            warning_style()
        } else {
            info_style()
        }
    }
}

fn lsm_style(lsm_status: &LsmStatus) -> Style {
    if lsm_status.major_lsm.is_some() {
        success_style()
    } else {
        warning_style()
    }
}

fn hardening_style(hardening_status: &HardeningStatus) -> Style {
    if hardening_status.warnings.is_empty() {
        success_style()
    } else {
        warning_style()
    }
}

fn overall_health_style(app: &OperatorTuiApp) -> Style {
    if !app.diagnostics.local_self_check.healthy
        || app.control.runtime.is_none()
        || app.lsm_status.major_lsm.is_none()
        || !app.hardening_status.warnings.is_empty()
    {
        warning_style()
    } else {
        success_style()
    }
}

fn validation_border_style(app: &OperatorTuiApp) -> Style {
    match validate_startup_config(&app.setup.draft) {
        Ok(()) => success_style(),
        Err(_) => error_style(),
    }
}

fn line_kv(language: UiLanguage, key: &str, value: String, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{}: ", label(language, key, key)),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, value_style),
    ])
}

fn audit_row_style(row: &AuditEventRow) -> Style {
    if !row.suppressed_reason.is_empty() {
        warning_style()
    } else if row.event_type == "mode_changed" {
        info_style()
    } else {
        Style::default()
    }
}

fn runtime_state_label(language: UiLanguage, control: &BotControlState) -> String {
    if control.runtime.is_some() {
        label(language, "control.runtime.running", "control.runtime.running").to_string()
    } else {
        label(language, "control.runtime.not_connected", "control.runtime.not_connected")
            .to_string()
    }
}

fn runtime_pid_label(language: UiLanguage, control: &BotControlState) -> String {
    control
        .runtime
        .as_ref()
        .map(|runtime| runtime.pid.to_string())
        .unwrap_or_else(|| label(language, "common.none", "common.none").to_string())
}

fn control_state_style(control: &BotControlState) -> Style {
    if control.runtime.is_some() {
        success_style()
    } else {
        warning_style()
    }
}

fn translate_check_name(language: UiLanguage, name: &str) -> String {
    match name {
        "audit_db_health" => {
            label(language, "check.audit_db_health", "check.audit_db_health").to_string()
        }
        "startup_config" => {
            label(language, "check.startup_config", "check.startup_config").to_string()
        }
        "hardening_script" => {
            label(language, "check.hardening_script", "check.hardening_script").to_string()
        }
        "pinned_hardened_toolchain" => {
            label(language, "check.pinned_hardened_toolchain", "check.pinned_hardened_toolchain")
                .to_string()
        }
        "hardening_verifier" => {
            label(language, "check.hardening_verifier", "check.hardening_verifier").to_string()
        }
        _ => name.to_string(),
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
    use tempfile::tempdir;

    use crate::audit::{AuditEventInput, AuditEventType, AuditStore, AuditStoreConfig};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::config::canonical_startup_config;

    fn sample_startup() -> LoadedStartupConfig {
        LoadedStartupConfig {
            app: canonical_startup_config(),
            config_path: PathBuf::from("config/oo-bot.yaml"),
            config_fingerprint: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            pseudo_id_hmac_key: None,
        }
    }

    fn render_snapshot(app: &OperatorTuiApp, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render_app(frame, app)).expect("draw snapshot");
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

    fn compact_snapshot(snapshot: &str) -> String {
        snapshot.chars().filter(|ch| !ch.is_whitespace()).collect()
    }

    fn sample_app(screen: Screen) -> OperatorTuiApp {
        let startup = sample_startup();
        let local_self_check = LocalSelfCheckReport { healthy: true, items: vec![] };
        OperatorTuiApp {
            status: "snapshot".to_string(),
            startup_created: false,
            saved_config: false,
            audit_limit: TUI_AUDIT_CAP,
            language: UiLanguage::English,
            screen,
            landing_screen: Screen::Dashboard,
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
                export_safe_policy_status: "export cap=50000 query cap=10000 pseudo-id=disabled"
                    .to_string(),
                confinement_state: "AppArmor profile docker-default".to_string(),
                writable_paths: vec![
                    "config: config".to_string(),
                    "audit: state/audit".to_string(),
                ],
                recommendations: vec![
                    "- prefer AppArmor".to_string(),
                    "- keep state persistent".to_string(),
                ],
            },
            control: BotControlState {
                socket_path: PathBuf::from("/run/oo-bot/control.sock"),
                runtime: Some(RuntimeControlStatus {
                    state: "running".to_string(),
                    pid: 4242,
                    started_at_unix: 1_700_000_000,
                    config_path: "config/oo-bot.yaml".to_string(),
                    config_fingerprint: "fingerprint".to_string(),
                    detector_backend: "morphological_reading".to_string(),
                    active_lsm: "apparmor".to_string(),
                    hardening_status: "ok".to_string(),
                    socket_path: "/run/oo-bot/control.sock".to_string(),
                }),
                status: "runtime control channel connected".to_string(),
                stop_armed: false,
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
        assert_eq!(
            snapshot,
            include_str!("../tests/snapshots/operator_dashboard_wide.txt").trim_end()
        );
    }

    #[test]
    fn setup_preview_snapshot_narrow() {
        let mut app = sample_app(Screen::Setup);
        app.setup.page = SetupPage::Preview;
        let snapshot = render_snapshot(&app, 72, 26);
        assert_eq!(
            snapshot,
            include_str!("../tests/snapshots/operator_setup_preview_narrow.txt").trim_end()
        );
    }

    #[test]
    fn audit_filter_ignores_raw_identifiers_and_sorts_descending() {
        let mut app = sample_app(Screen::Audit);
        app.audit.rows = vec![
            AuditEventRow {
                event_id: 2,
                pseudo_user_id: "raw-user-should-not-match".to_string(),
                matched_readings_json: "[\"secret-reading\"]".to_string(),
                ..app.audit.rows[0].clone()
            },
            AuditEventRow {
                event_id: 7,
                event_type: "suppressed".to_string(),
                detector_backend: "sandbox_plugin".to_string(),
                selected_action: "reaction".to_string(),
                suppressed_reason: "duplicate_guard".to_string(),
                mode: "observe_only".to_string(),
                ..app.audit.rows[0].clone()
            },
        ];
        app.audit.search = "raw-user-should-not-match".to_string();
        assert!(filtered_audit_rows(&app.audit).is_empty());

        app.audit.search = "duplicate_guard".to_string();
        let filtered = filtered_audit_rows(&app.audit);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event_id, 7);

        app.audit.search.clear();
        app.audit.sort = AuditSort::NewestFirst;
        let filtered = filtered_audit_rows(&app.audit);
        assert_eq!(filtered[0].event_id, 7);
        assert_eq!(filtered[1].event_id, 2);
    }

    #[test]
    fn audit_render_hides_raw_identifiers_and_message_like_content() {
        let mut app = sample_app(Screen::Audit);
        app.audit.rows[0].pseudo_guild_id = "guild-raw-id".to_string();
        app.audit.rows[0].pseudo_channel_id = "channel-raw-id".to_string();
        app.audit.rows[0].pseudo_user_id = "user-raw-id".to_string();
        app.audit.rows[0].pseudo_message_id = "message-raw-id".to_string();
        app.audit.rows[0].matched_readings_json = "[\"super-secret-reading\"]".to_string();
        app.audit.rows[0].suspicious_flags_json = "[\"raw-message-fragment\"]".to_string();

        let snapshot = render_snapshot(&app, 100, 24);
        assert!(snapshot.contains("backend=morphological_reading"));
        assert!(!snapshot.contains("guild-raw-id"));
        assert!(!snapshot.contains("channel-raw-id"));
        assert!(!snapshot.contains("user-raw-id"));
        assert!(!snapshot.contains("message-raw-id"));
        assert!(!snapshot.contains("super-secret-reading"));
        assert!(!snapshot.contains("raw-message-fragment"));
    }

    #[test]
    fn welcome_screen_shows_banner_and_language_choices() {
        let app = sample_app(Screen::Welcome);
        let snapshot = render_snapshot(&app, 100, 24);
        let compact = compact_snapshot(&snapshot);
        assert!(compact.contains("WELCOMETOOODETECTION"));
        assert!(compact.contains("RESPONSEANALYZER"));
        assert!(compact.contains("English"));
        assert!(compact.contains("日本語"));
    }

    #[test]
    fn japanese_language_renders_polished_labels() {
        let mut app = sample_app(Screen::Diagnostics);
        app.language = UiLanguage::Japanese;
        relocalize_app(&mut app);
        let snapshot = render_snapshot(&app, 100, 24);
        let compact = compact_snapshot(&snapshot);
        assert!(compact.contains("診断サマリー"));
        assert!(compact.contains("有効なLSM"));
        assert!(compact.contains("自己診断の詳細"));
    }

    #[test]
    fn setup_edit_can_update_target_readings_from_csv() {
        let mut app = sample_app(Screen::Setup);
        apply_custom_field_input(
            &mut app.setup,
            SetupField::TargetReadings,
            "おおき, かみ ,オオキ",
            UiLanguage::Japanese,
        )
        .expect("apply target readings edit");

        assert_eq!(
            app.setup.draft.detector.target_readings,
            vec!["おおき".to_string(), "かみ".to_string(), "オオキ".to_string()]
        );
    }

    #[test]
    fn audit_limit_is_preserved_across_reload_paths() {
        let dir = tempdir().expect("temp dir");
        let sqlite_path = dir.path().join("audit.sqlite3");
        let cfg = AuditStoreConfig {
            sqlite_path: sqlite_path.clone(),
            busy_timeout_ms: 1000,
            export_max_rows: 1000,
            query_max_rows: 1000,
        };

        let mut store = AuditStore::open_rw(&cfg, None).expect("open rw audit store");
        for _ in 0..3 {
            let input = AuditEventInput {
                event_type: AuditEventType::ModeChanged,
                detector_backend: "morphological_reading".to_string(),
                selected_action: "noop".to_string(),
                mode: "normal".to_string(),
                ..AuditEventInput::default()
            };
            let _ = store.record_event(&input).expect("insert event");
        }
        drop(store);

        let mut app = sample_app(Screen::Audit);
        app.startup.app.audit.sqlite_path = sqlite_path;
        app.startup.app.audit.query_max_rows = 1000;
        app.startup.app.audit.busy_timeout_ms = 1000;
        app.audit_limit = 1;
        app.audit = load_audit_state(&app.startup, app.audit_limit, app.language);
        assert_eq!(app.audit.rows.len(), 1);

        relocalize_app(&mut app);
        assert_eq!(app.audit.rows.len(), 1);

        refresh_runtime_snapshot(&mut app);
        assert_eq!(app.audit.rows.len(), 1);

        handle_audit_normal_keys(&mut app, KeyCode::Char('r')).expect("audit reload");
        assert_eq!(app.audit.rows.len(), 1);
    }
}
