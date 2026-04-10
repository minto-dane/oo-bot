use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use arrow_array::{RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use hmac::{Hmac, Mac};
use parquet::arrow::arrow_writer::ArrowWriter;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::warn;

type HmacSha256 = Hmac<Sha256>;

pub const SCHEMA_VERSION: i64 = 1;
const MANIFEST_INTERVAL: i64 = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    ProcessStart,
    ProcessShutdown,
    ConfigLoaded,
    ConfigSignatureVerified,
    ConfigSignatureFailed,
    DetectorMatch,
    SpecialPhraseMatch,
    ResponseCompiled,
    ActionSent,
    ActionSuppressed,
    BreakerOpen,
    BreakerClose,
    ModeChanged,
    SuspiciousInputDetected,
    DuplicateSuppressed,
    CooldownSuppressed,
    RateLimitSuppressed,
    SandboxFault,
    AuditIntegrityWarning,
    AuditIntegrityDegraded,
    ExportStarted,
    ExportFinished,
}

impl AuditEventType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ProcessStart => "process_start",
            Self::ProcessShutdown => "process_shutdown",
            Self::ConfigLoaded => "config_loaded",
            Self::ConfigSignatureVerified => "config_signature_verified",
            Self::ConfigSignatureFailed => "config_signature_failed",
            Self::DetectorMatch => "detector_match",
            Self::SpecialPhraseMatch => "special_phrase_match",
            Self::ResponseCompiled => "response_compiled",
            Self::ActionSent => "action_sent",
            Self::ActionSuppressed => "action_suppressed",
            Self::BreakerOpen => "breaker_open",
            Self::BreakerClose => "breaker_close",
            Self::ModeChanged => "mode_changed",
            Self::SuspiciousInputDetected => "suspicious_input_detected",
            Self::DuplicateSuppressed => "duplicate_suppressed",
            Self::CooldownSuppressed => "cooldown_suppressed",
            Self::RateLimitSuppressed => "rate_limit_suppressed",
            Self::SandboxFault => "sandbox_fault",
            Self::AuditIntegrityWarning => "audit_integrity_warning",
            Self::AuditIntegrityDegraded => "audit_integrity_degraded",
            Self::ExportStarted => "export_started",
            Self::ExportFinished => "export_finished",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditStoreConfig {
    pub sqlite_path: PathBuf,
    pub busy_timeout_ms: u64,
    pub export_max_rows: usize,
    pub query_max_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventInput {
    pub event_type: AuditEventType,
    pub binary_version: String,
    pub config_fingerprint: String,
    pub detector_backend: String,
    pub matched_readings: Vec<String>,
    pub sequence_hits: usize,
    pub kanji_hits: usize,
    pub total_count: usize,
    pub special_phrase_hit: bool,
    pub selected_action: String,
    pub suppressed_reason: Option<String>,
    pub mode: String,
    pub active_lsm: String,
    pub hardening_status: String,
    pub processing_time_ms: u64,
    pub message_length: usize,
    pub normalized_length: usize,
    pub token_count: usize,
    pub suspicious_flags: Vec<String>,
    pub truncated_flag: bool,
    pub guild_id: Option<u64>,
    pub channel_id: Option<u64>,
    pub user_id: Option<u64>,
    pub message_id: Option<u64>,
}

impl Default for AuditEventInput {
    fn default() -> Self {
        Self {
            event_type: AuditEventType::DetectorMatch,
            binary_version: env!("CARGO_PKG_VERSION").to_string(),
            config_fingerprint: "unknown".to_string(),
            detector_backend: "unknown".to_string(),
            matched_readings: vec![],
            sequence_hits: 0,
            kanji_hits: 0,
            total_count: 0,
            special_phrase_hit: false,
            selected_action: "noop".to_string(),
            suppressed_reason: None,
            mode: "normal".to_string(),
            active_lsm: "unknown".to_string(),
            hardening_status: "unknown".to_string(),
            processing_time_ms: 0,
            message_length: 0,
            normalized_length: 0,
            token_count: 0,
            suspicious_flags: vec![],
            truncated_flag: false,
            guild_id: None,
            channel_id: None,
            user_id: None,
            message_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRow {
    pub event_id: i64,
    pub ts_utc: String,
    pub event_type: String,
    pub schema_version: i64,
    pub binary_version: String,
    pub config_fingerprint: String,
    pub detector_backend: String,
    pub matched_readings_json: String,
    pub sequence_hits: i64,
    pub kanji_hits: i64,
    pub total_count: i64,
    pub special_phrase_hit: i64,
    pub selected_action: String,
    pub suppressed_reason: String,
    pub mode: String,
    pub active_lsm: String,
    pub hardening_status: String,
    pub processing_time_ms: i64,
    pub message_length: i64,
    pub normalized_length: i64,
    pub token_count: i64,
    pub suspicious_flags_json: String,
    pub truncated_flag: i64,
    pub pseudo_guild_id: String,
    pub pseudo_channel_id: String,
    pub pseudo_user_id: String,
    pub pseudo_message_id: String,
    pub prev_hash: String,
    pub row_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditQueryFilter {
    pub start_ts_utc: Option<String>,
    pub end_ts_utc: Option<String>,
    pub event_type: Option<String>,
    pub detector_backend: Option<String>,
    pub suppressed_reason: Option<String>,
    pub mode: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total: usize,
    pub by_event_type: BTreeMap<String, usize>,
    pub by_backend: BTreeMap<String, usize>,
    pub by_suppressed_reason: BTreeMap<String, usize>,
    pub by_mode: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub checked_rows: usize,
    pub broken_rows: usize,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Jsonl,
    Csv,
    Parquet,
}

pub struct AuditStore {
    conn: Connection,
    mode: AuditStoreMode,
    pseudo_id_hmac_key: Option<Vec<u8>>,
    export_max_rows: usize,
    query_max_rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuditStoreMode {
    ReadOnly,
    ReadWrite,
}

impl AuditStore {
    pub fn open_rw(
        cfg: &AuditStoreConfig,
        pseudo_id_hmac_key: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        if let Some(parent) = cfg.sqlite_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }

        let conn = Connection::open(&cfg.sqlite_path).map_err(|err| err.to_string())?;
        apply_pragmas(&conn, cfg.busy_timeout_ms, false)?;
        migrate(&conn)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(&cfg.sqlite_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(&cfg.sqlite_path, perms);
            }
        }

        Ok(Self {
            conn,
            mode: AuditStoreMode::ReadWrite,
            pseudo_id_hmac_key,
            export_max_rows: cfg.export_max_rows,
            query_max_rows: cfg.query_max_rows,
        })
    }

    pub fn open_ro(cfg: &AuditStoreConfig) -> Result<Self, String> {
        let conn = Connection::open_with_flags(&cfg.sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|err| err.to_string())?;
        apply_pragmas(&conn, cfg.busy_timeout_ms, true)?;

        Ok(Self {
            conn,
            mode: AuditStoreMode::ReadOnly,
            pseudo_id_hmac_key: None,
            export_max_rows: cfg.export_max_rows,
            query_max_rows: cfg.query_max_rows,
        })
    }

    pub fn record_config_snapshot(
        &mut self,
        config_fingerprint: &str,
        config_json: &str,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        self.conn
            .execute(
                "INSERT INTO config_snapshots (ts_utc, config_fingerprint, config_json) VALUES (?1, ?2, ?3)",
                params![now_utc_string()?, config_fingerprint, config_json],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn record_event(&mut self, input: &AuditEventInput) -> Result<i64, String> {
        self.ensure_writable()?;
        let prev_hash: String = self
            .conn
            .query_row(
                "SELECT row_hash FROM audit_events ORDER BY event_id DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?
            .unwrap_or_else(|| "GENESIS".to_string());

        let ts_utc = now_utc_string()?;
        let matched_readings_json =
            serde_json::to_string(&input.matched_readings).map_err(|err| err.to_string())?;
        let suspicious_flags_json =
            serde_json::to_string(&input.suspicious_flags).map_err(|err| err.to_string())?;

        let pseudo_guild_id = self.pseudo_id(input.guild_id);
        let pseudo_channel_id = self.pseudo_id(input.channel_id);
        let pseudo_user_id = self.pseudo_id(input.user_id);
        let pseudo_message_id = self.pseudo_id(input.message_id);

        let payload = serde_json::json!({
            "ts_utc": ts_utc,
            "event_type": input.event_type.as_str(),
            "schema_version": SCHEMA_VERSION,
            "binary_version": input.binary_version,
            "config_fingerprint": input.config_fingerprint,
            "detector_backend": input.detector_backend,
            "matched_readings_json": matched_readings_json,
            "sequence_hits": input.sequence_hits,
            "kanji_hits": input.kanji_hits,
            "total_count": input.total_count,
            "special_phrase_hit": input.special_phrase_hit,
            "selected_action": input.selected_action,
            "suppressed_reason": input.suppressed_reason,
            "mode": input.mode,
            "active_lsm": input.active_lsm,
            "hardening_status": input.hardening_status,
            "processing_time_ms": input.processing_time_ms,
            "message_length": input.message_length,
            "normalized_length": input.normalized_length,
            "token_count": input.token_count,
            "suspicious_flags_json": suspicious_flags_json,
            "truncated_flag": input.truncated_flag,
            "pseudo_guild_id": pseudo_guild_id,
            "pseudo_channel_id": pseudo_channel_id,
            "pseudo_user_id": pseudo_user_id,
            "pseudo_message_id": pseudo_message_id,
        });

        let normalized_payload_json =
            serde_json::to_string(&payload).map_err(|err| err.to_string())?;
        let row_hash = compute_row_hash(&prev_hash, &normalized_payload_json);

        self.conn
            .execute(
                "INSERT INTO audit_events (
                    ts_utc,
                    event_type,
                    schema_version,
                    binary_version,
                    config_fingerprint,
                    detector_backend,
                    matched_readings_json,
                    sequence_hits,
                    kanji_hits,
                    total_count,
                    special_phrase_hit,
                    selected_action,
                    suppressed_reason,
                    mode,
                    active_lsm,
                    hardening_status,
                    processing_time_ms,
                    message_length,
                    normalized_length,
                    token_count,
                    suspicious_flags_json,
                    truncated_flag,
                    pseudo_guild_id,
                    pseudo_channel_id,
                    pseudo_user_id,
                    pseudo_message_id,
                    prev_hash,
                    row_hash,
                    normalized_payload_json
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                    ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29
                )",
                params![
                    ts_utc,
                    input.event_type.as_str(),
                    SCHEMA_VERSION,
                    input.binary_version,
                    input.config_fingerprint,
                    input.detector_backend,
                    matched_readings_json,
                    input.sequence_hits as i64,
                    input.kanji_hits as i64,
                    input.total_count as i64,
                    i64::from(input.special_phrase_hit),
                    input.selected_action,
                    input.suppressed_reason.clone().unwrap_or_default(),
                    input.mode,
                    input.active_lsm,
                    input.hardening_status,
                    input.processing_time_ms as i64,
                    input.message_length as i64,
                    input.normalized_length as i64,
                    input.token_count as i64,
                    suspicious_flags_json,
                    i64::from(input.truncated_flag),
                    pseudo_guild_id,
                    pseudo_channel_id,
                    pseudo_user_id,
                    pseudo_message_id,
                    prev_hash,
                    row_hash,
                    normalized_payload_json,
                ],
            )
            .map_err(|err| err.to_string())?;

        let event_id = self.conn.last_insert_rowid();
        self.maybe_create_manifest(event_id)?;
        Ok(event_id)
    }

    pub fn tail(&self, filter: &AuditQueryFilter) -> Result<Vec<AuditEventRow>, String> {
        let mut events = self.query_events_capped(filter)?;
        events.sort_by_key(|event| Reverse(event.event_id));
        let cap = filter.limit.unwrap_or(100).min(self.query_max_rows);
        events.truncate(cap);
        Ok(events)
    }

    pub fn inspect(&self, event_id: i64) -> Result<Option<AuditEventRow>, String> {
        let mut statement = self
            .conn
            .prepare("SELECT * FROM audit_events WHERE event_id = ?1 LIMIT 1")
            .map_err(|err| err.to_string())?;
        let row = statement
            .query_row(params![event_id], map_audit_event_row)
            .optional()
            .map_err(|err| err.to_string())?;
        Ok(row)
    }

    pub fn stats(&self, filter: &AuditQueryFilter) -> Result<AuditStats, String> {
        let events = self.query_events_capped(filter)?;
        let mut stats = AuditStats {
            total: events.len(),
            by_event_type: BTreeMap::new(),
            by_backend: BTreeMap::new(),
            by_suppressed_reason: BTreeMap::new(),
            by_mode: BTreeMap::new(),
        };

        for event in events {
            *stats.by_event_type.entry(event.event_type).or_insert(0) += 1;
            *stats.by_backend.entry(event.detector_backend).or_insert(0) += 1;
            if !event.suppressed_reason.is_empty() {
                *stats.by_suppressed_reason.entry(event.suppressed_reason).or_insert(0) += 1;
            }
            *stats.by_mode.entry(event.mode).or_insert(0) += 1;
        }

        Ok(stats)
    }

    pub fn verify(
        &self,
        start_event_id: Option<i64>,
        end_event_id: Option<i64>,
    ) -> Result<VerifyReport, String> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT event_id, prev_hash, row_hash, normalized_payload_json
                 FROM audit_events
                 WHERE (?1 IS NULL OR event_id >= ?1)
                   AND (?2 IS NULL OR event_id <= ?2)
                 ORDER BY event_id ASC",
            )
            .map_err(|err| err.to_string())?;
        let mut rows = statement
            .query(params![start_event_id, end_event_id])
            .map_err(|err| err.to_string())?;

        let mut checked = 0usize;
        let mut broken = 0usize;
        let mut details = Vec::new();
        let mut expected_prev = if let Some(start_id) = start_event_id {
            if start_id <= 1 {
                "GENESIS".to_string()
            } else {
                self.conn
                    .query_row(
                        "SELECT row_hash FROM audit_events WHERE event_id = ?1 LIMIT 1",
                        params![start_id - 1],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|err| err.to_string())?
                    .ok_or_else(|| {
                        format!(
                            "failed to initialize verify chain: missing row_hash for event_id={}",
                            start_id - 1
                        )
                    })?
            }
        } else {
            "GENESIS".to_string()
        };

        while let Some(row) = rows.next().map_err(|err| err.to_string())? {
            checked = checked.saturating_add(1);
            let event_id: i64 = row.get(0).map_err(|err| err.to_string())?;
            let prev_hash: String = row.get(1).map_err(|err| err.to_string())?;
            let row_hash: String = row.get(2).map_err(|err| err.to_string())?;
            let payload_json: String = row.get(3).map_err(|err| err.to_string())?;

            if prev_hash != expected_prev {
                broken = broken.saturating_add(1);
                details.push(format!("event_id={event_id} prev_hash mismatch"));
            }

            let recomputed = compute_row_hash(&prev_hash, &payload_json);
            if recomputed != row_hash {
                broken = broken.saturating_add(1);
                details.push(format!("event_id={event_id} row_hash mismatch"));
            }

            expected_prev = row_hash;
        }

        Ok(VerifyReport { checked_rows: checked, broken_rows: broken, details })
    }

    pub fn export(
        &self,
        format: ExportFormat,
        out_path: &Path,
        filter: &AuditQueryFilter,
    ) -> Result<usize, String> {
        validate_export_output_path(out_path)?;

        let mut filter = filter.clone();
        filter.limit = Some(filter.limit.unwrap_or(self.export_max_rows).min(self.export_max_rows));

        let mut rows = self.query_events_capped(&filter)?;
        rows.sort_by_key(|row| row.event_id);

        match format {
            ExportFormat::Jsonl => export_jsonl(out_path, &rows)?,
            ExportFormat::Csv => export_csv(out_path, &rows)?,
            ExportFormat::Parquet => export_parquet(out_path, &rows)?,
        }

        Ok(rows.len())
    }

    fn query_events_capped(&self, filter: &AuditQueryFilter) -> Result<Vec<AuditEventRow>, String> {
        let mut statement = self
            .conn
            .prepare("SELECT * FROM audit_events ORDER BY event_id DESC LIMIT ?1")
            .map_err(|err| err.to_string())?;
        let raw_limit = filter.limit.unwrap_or(self.query_max_rows).min(self.query_max_rows);
        let mapped = statement
            .query_map(params![raw_limit as i64], map_audit_event_row)
            .map_err(|err| err.to_string())?;

        let mut rows = Vec::new();
        for row in mapped {
            let row = row.map_err(|err| err.to_string())?;
            if !passes_filter(&row, filter) {
                continue;
            }
            rows.push(row);
        }

        Ok(rows)
    }

    fn pseudo_id(&self, raw_id: Option<u64>) -> String {
        let Some(raw_id) = raw_id else {
            return String::new();
        };
        let Some(key) = self.pseudo_id_hmac_key.as_ref() else {
            return String::new();
        };

        let mut mac = match HmacSha256::new_from_slice(key) {
            Ok(mac) => mac,
            Err(err) => {
                warn!(
                    error = %err,
                    key_len = key.len(),
                    "failed to initialize pseudo-id HMAC; pseudo identifier omitted"
                );
                return String::new();
            }
        };
        mac.update(raw_id.to_string().as_bytes());
        let digest = mac.finalize().into_bytes();
        hex::encode(digest)
    }

    fn maybe_create_manifest(&mut self, last_event_id: i64) -> Result<(), String> {
        if last_event_id % MANIFEST_INTERVAL != 0 {
            return Ok(());
        }

        let from_event = last_event_id - MANIFEST_INTERVAL + 1;
        let mut statement = self
            .conn
            .prepare(
                "SELECT row_hash FROM audit_events WHERE event_id >= ?1 AND event_id <= ?2 ORDER BY event_id ASC",
            )
            .map_err(|err| err.to_string())?;

        let mut rows =
            statement.query(params![from_event, last_event_id]).map_err(|err| err.to_string())?;

        let mut concatenated = String::new();
        while let Some(row) = rows.next().map_err(|err| err.to_string())? {
            let row_hash: String = row.get(0).map_err(|err| err.to_string())?;
            concatenated.push_str(&row_hash);
        }

        let manifest_hash = hex::encode(Sha256::digest(concatenated.as_bytes()));
        self.conn
            .execute(
                "INSERT INTO audit_manifests (ts_utc, start_event_id, end_event_id, manifest_hash) VALUES (?1, ?2, ?3, ?4)",
                params![now_utc_string()?, from_event, last_event_id, manifest_hash],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.mode == AuditStoreMode::ReadOnly {
            return Err("audit store is read-only".to_string());
        }
        Ok(())
    }
}

fn apply_pragmas(conn: &Connection, busy_timeout_ms: u64, read_only: bool) -> Result<(), String> {
    if !read_only {
        conn.pragma_update(None, "journal_mode", "WAL").map_err(|err| err.to_string())?;
        conn.pragma_update(None, "wal_autocheckpoint", "10000").map_err(|err| err.to_string())?;
    }
    conn.pragma_update(None, "foreign_keys", "ON").map_err(|err| err.to_string())?;
    conn.pragma_update(None, "trusted_schema", "OFF").map_err(|err| err.to_string())?;
    if read_only {
        conn.pragma_update(None, "query_only", "ON").map_err(|err| err.to_string())?;
    }
    conn.busy_timeout(std::time::Duration::from_millis(busy_timeout_ms))
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS audit_events (
            event_id INTEGER PRIMARY KEY,
            ts_utc TEXT NOT NULL,
            event_type TEXT NOT NULL,
            schema_version INTEGER NOT NULL,
            binary_version TEXT NOT NULL,
            config_fingerprint TEXT NOT NULL,
            detector_backend TEXT NOT NULL,
            matched_readings_json TEXT NOT NULL,
            sequence_hits INTEGER NOT NULL,
            kanji_hits INTEGER NOT NULL,
            total_count INTEGER NOT NULL,
            special_phrase_hit INTEGER NOT NULL,
            selected_action TEXT NOT NULL,
            suppressed_reason TEXT NOT NULL,
            mode TEXT NOT NULL,
            active_lsm TEXT NOT NULL,
            hardening_status TEXT NOT NULL,
            processing_time_ms INTEGER NOT NULL,
            message_length INTEGER NOT NULL,
            normalized_length INTEGER NOT NULL,
            token_count INTEGER NOT NULL,
            suspicious_flags_json TEXT NOT NULL,
            truncated_flag INTEGER NOT NULL,
            pseudo_guild_id TEXT NOT NULL,
            pseudo_channel_id TEXT NOT NULL,
            pseudo_user_id TEXT NOT NULL,
            pseudo_message_id TEXT NOT NULL,
            prev_hash TEXT NOT NULL,
            row_hash TEXT NOT NULL,
            normalized_payload_json TEXT NOT NULL
        ) STRICT;

        CREATE TABLE IF NOT EXISTS audit_manifests (
            manifest_id INTEGER PRIMARY KEY,
            ts_utc TEXT NOT NULL,
            start_event_id INTEGER NOT NULL,
            end_event_id INTEGER NOT NULL,
            manifest_hash TEXT NOT NULL
        ) STRICT;

        CREATE TABLE IF NOT EXISTS config_snapshots (
            snapshot_id INTEGER PRIMARY KEY,
            ts_utc TEXT NOT NULL,
            config_fingerprint TEXT NOT NULL,
            config_json TEXT NOT NULL
        ) STRICT;

        CREATE TABLE IF NOT EXISTS exports_history (
            export_id INTEGER PRIMARY KEY,
            ts_utc TEXT NOT NULL,
            format TEXT NOT NULL,
            output_path TEXT NOT NULL,
            exported_rows INTEGER NOT NULL,
            filter_json TEXT NOT NULL
        ) STRICT;

        CREATE TABLE IF NOT EXISTS integrity_checks (
            check_id INTEGER PRIMARY KEY,
            ts_utc TEXT NOT NULL,
            checked_rows INTEGER NOT NULL,
            broken_rows INTEGER NOT NULL,
            details_json TEXT NOT NULL
        ) STRICT;

        CREATE TRIGGER IF NOT EXISTS audit_events_no_update
        BEFORE UPDATE ON audit_events
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS audit_events_no_delete
        BEFORE DELETE ON audit_events
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS audit_manifests_no_update
        BEFORE UPDATE ON audit_manifests
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS audit_manifests_no_delete
        BEFORE DELETE ON audit_manifests
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS config_snapshots_no_update
        BEFORE UPDATE ON config_snapshots
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS config_snapshots_no_delete
        BEFORE DELETE ON config_snapshots
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS exports_history_no_update
        BEFORE UPDATE ON exports_history
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS exports_history_no_delete
        BEFORE DELETE ON exports_history
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS integrity_checks_no_update
        BEFORE UPDATE ON integrity_checks
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;

        CREATE TRIGGER IF NOT EXISTS integrity_checks_no_delete
        BEFORE DELETE ON integrity_checks
        BEGIN
            SELECT RAISE(ABORT, 'append-only table');
        END;
        ",
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

fn map_audit_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEventRow> {
    Ok(AuditEventRow {
        event_id: row.get("event_id")?,
        ts_utc: row.get("ts_utc")?,
        event_type: row.get("event_type")?,
        schema_version: row.get("schema_version")?,
        binary_version: row.get("binary_version")?,
        config_fingerprint: row.get("config_fingerprint")?,
        detector_backend: row.get("detector_backend")?,
        matched_readings_json: row.get("matched_readings_json")?,
        sequence_hits: row.get("sequence_hits")?,
        kanji_hits: row.get("kanji_hits")?,
        total_count: row.get("total_count")?,
        special_phrase_hit: row.get("special_phrase_hit")?,
        selected_action: row.get("selected_action")?,
        suppressed_reason: row.get("suppressed_reason")?,
        mode: row.get("mode")?,
        active_lsm: row.get("active_lsm")?,
        hardening_status: row.get("hardening_status")?,
        processing_time_ms: row.get("processing_time_ms")?,
        message_length: row.get("message_length")?,
        normalized_length: row.get("normalized_length")?,
        token_count: row.get("token_count")?,
        suspicious_flags_json: row.get("suspicious_flags_json")?,
        truncated_flag: row.get("truncated_flag")?,
        pseudo_guild_id: row.get("pseudo_guild_id")?,
        pseudo_channel_id: row.get("pseudo_channel_id")?,
        pseudo_user_id: row.get("pseudo_user_id")?,
        pseudo_message_id: row.get("pseudo_message_id")?,
        prev_hash: row.get("prev_hash")?,
        row_hash: row.get("row_hash")?,
    })
}

fn passes_filter(row: &AuditEventRow, filter: &AuditQueryFilter) -> bool {
    if let Some(start) = &filter.start_ts_utc {
        if row.ts_utc < *start {
            return false;
        }
    }
    if let Some(end) = &filter.end_ts_utc {
        if row.ts_utc > *end {
            return false;
        }
    }
    if let Some(event_type) = &filter.event_type {
        if &row.event_type != event_type {
            return false;
        }
    }
    if let Some(backend) = &filter.detector_backend {
        if &row.detector_backend != backend {
            return false;
        }
    }
    if let Some(reason) = &filter.suppressed_reason {
        if &row.suppressed_reason != reason {
            return false;
        }
    }
    if let Some(mode) = &filter.mode {
        if &row.mode != mode {
            return false;
        }
    }
    true
}

fn now_utc_string() -> Result<String, String> {
    OffsetDateTime::now_utc().format(&Rfc3339).map_err(|err| err.to_string())
}

fn compute_row_hash(prev_hash: &str, normalized_payload_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(normalized_payload_json.as_bytes());
    hex::encode(hasher.finalize())
}

fn validate_export_output_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("export path is empty".to_string());
    }

    if path.components().any(|component| component == Component::ParentDir) {
        return Err("export path must not contain '..'".to_string());
    }

    if path.is_dir() {
        return Err("export path points to a directory".to_string());
    }

    if path.exists() {
        return Err("export destination already exists; refusing to overwrite".to_string());
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            if fs::symlink_metadata(parent)
                .map(|meta| meta.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err("export parent directory must not be a symlink".to_string());
            }
        }
    }

    Ok(())
}

fn secure_create_output_file(path: &Path) -> Result<fs::File, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|err| err.to_string())
    }

    #[cfg(not(unix))]
    {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|err| err.to_string())
    }
}

fn export_jsonl(path: &Path, rows: &[AuditEventRow]) -> Result<(), String> {
    let file = secure_create_output_file(path)?;
    let mut writer = BufWriter::new(file);

    for row in rows {
        let line = serde_json::to_string(row).map_err(|err| err.to_string())?;
        writer
            .write_all(line.as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .map_err(|err| err.to_string())?;
    }

    writer.flush().map_err(|err| err.to_string())
}

fn export_csv(path: &Path, rows: &[AuditEventRow]) -> Result<(), String> {
    let file = secure_create_output_file(path)?;
    let mut writer = csv::Writer::from_writer(file);
    for row in rows {
        writer.serialize(row).map_err(|err| err.to_string())?;
    }
    writer.flush().map_err(|err| err.to_string())
}

fn export_parquet(path: &Path, rows: &[AuditEventRow]) -> Result<(), String> {
    let schema = Arc::new(Schema::new(vec![Field::new("event_json", DataType::Utf8, false)]));
    let json_values: Vec<String> = rows
        .iter()
        .map(|row| {
            serde_json::to_string(row).map_err(|err| {
                format!(
                    "failed to serialize audit row for parquet export (event_id={}): {}",
                    row.event_id, err
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let batch =
        RecordBatch::try_new(schema.clone(), vec![Arc::new(StringArray::from(json_values))])
            .map_err(|err| err.to_string())?;

    let file = secure_create_output_file(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, None).map_err(|err| err.to_string())?;
    writer.write(&batch).map_err(|err| err.to_string())?;
    writer.close().map_err(|err| err.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::{
        AuditEventInput, AuditEventType, AuditQueryFilter, AuditStore, AuditStoreConfig,
        ExportFormat,
    };

    #[test]
    fn hash_chain_verify_detects_tamper() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("audit.sqlite3");

        let mut store = AuditStore::open_rw(
            &AuditStoreConfig {
                sqlite_path: db_path.clone(),
                busy_timeout_ms: 10,
                export_max_rows: 100,
                query_max_rows: 100,
            },
            Some(b"test-key".to_vec()),
        )
        .expect("open");

        let input = AuditEventInput {
            event_type: AuditEventType::ProcessStart,
            ..AuditEventInput::default()
        };
        let _ = store.record_event(&input).expect("insert event");

        drop(store);

        let conn = Connection::open(&db_path).expect("open sqlite");
        conn.execute("UPDATE audit_events SET selected_action='tampered' WHERE event_id=1", [])
            .expect_err("append-only update should fail");

        drop(conn);

        let store = AuditStore::open_ro(&AuditStoreConfig {
            sqlite_path: db_path,
            busy_timeout_ms: 10,
            export_max_rows: 100,
            query_max_rows: 100,
        })
        .expect("open ro");

        let report = store.verify(None, None).expect("verify");
        assert_eq!(report.broken_rows, 0);
    }

    #[test]
    fn export_safe_formats_are_written() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("audit.sqlite3");

        let mut store = AuditStore::open_rw(
            &AuditStoreConfig {
                sqlite_path: db_path,
                busy_timeout_ms: 10,
                export_max_rows: 100,
                query_max_rows: 100,
            },
            None,
        )
        .expect("open");

        let _ = store.record_event(&AuditEventInput::default()).expect("insert");

        let jsonl = dir.path().join("events.jsonl");
        let csv = dir.path().join("events.csv");
        let parquet = dir.path().join("events.parquet");

        let filter = AuditQueryFilter::default();
        assert_eq!(store.export(ExportFormat::Jsonl, &jsonl, &filter).expect("jsonl"), 1);
        assert_eq!(store.export(ExportFormat::Csv, &csv, &filter).expect("csv"), 1);
        assert_eq!(store.export(ExportFormat::Parquet, &parquet, &filter).expect("parquet"), 1);

        assert!(jsonl.exists());
        assert!(csv.exists());
        assert!(parquet.exists());
    }
}
