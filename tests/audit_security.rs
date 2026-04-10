use discord_oo_bot::audit::{AuditEventInput, AuditStore, AuditStoreConfig};
use tempfile::tempdir;

#[test]
fn raw_identifiers_are_not_stored() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("audit.sqlite3");

    let mut store = AuditStore::open_rw(
        &AuditStoreConfig {
            sqlite_path: db_path,
            busy_timeout_ms: 100,
            export_max_rows: 100,
            query_max_rows: 100,
        },
        Some(b"pseudo-key".to_vec()),
    )
    .expect("open audit");

    let event = AuditEventInput {
        guild_id: Some(1001),
        channel_id: Some(1002),
        user_id: Some(1003),
        message_id: Some(1004),
        ..AuditEventInput::default()
    };

    let id = store.record_event(&event).expect("insert");
    let row = store.inspect(id).expect("inspect").expect("row");

    assert_ne!(row.pseudo_guild_id, "1001");
    assert_ne!(row.pseudo_channel_id, "1002");
    assert_ne!(row.pseudo_user_id, "1003");
    assert_ne!(row.pseudo_message_id, "1004");
    assert!(!row.pseudo_guild_id.is_empty());
}
