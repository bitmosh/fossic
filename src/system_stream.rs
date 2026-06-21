use crate::{
    cce::derive_event_id,
    schema::now_us,
    types::EventId,
};
use rusqlite::{Connection, TransactionBehavior};
use std::path::Path;

const SYSTEM_STREAM: &str = "_fossic/system";
const SYSTEM_BRANCH: &str = "main";

/// Owns a dedicated SQLite connection for writing to the `_fossic/system` stream.
///
/// Separate from the store write mutex and read pool — the dispatcher thread
/// holds this exclusively so system events never contend with user appends.
/// All emission methods are best-effort: errors are logged and silently dropped.
pub struct SystemStreamWriter {
    conn: Connection,
}

impl SystemStreamWriter {
    /// Open a dedicated connection to `db_path`. Returns `None` if the connection
    /// fails (with a WARN log); callers must tolerate the absence of a writer.
    pub fn new(db_path: &Path) -> Option<Self> {
        match Connection::open(db_path) {
            Ok(conn) => {
                let _ = conn.execute_batch(
                    "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 30000;",
                );
                Some(SystemStreamWriter { conn })
            }
            Err(e) => {
                eprintln!("[WARN fossic] SystemStreamWriter: failed to open: {e}");
                None
            }
        }
    }

    /// Write one event to `_fossic/system`. The event_id is derived internally
    /// via CCE — callers do not supply it. `type_version` is always 1; no
    /// causation_id is set. Errors are silently dropped (best-effort delivery).
    pub fn emit(
        &mut self,
        event_type: &str,
        payload: &serde_json::Value,
        indexed_tags: Option<&serde_json::Value>,
    ) {
        let payload_bytes = match rmp_serde::to_vec(payload) {
            Ok(b) => b,
            Err(_) => return,
        };

        let event_id_bytes = match derive_event_id(event_type, 1, None, payload) {
            Ok(b) => b,
            Err(_) => return,
        };
        let event_id = EventId::from_bytes(event_id_bytes);
        let ts = now_us();

        let indexed_tags_json = indexed_tags.map(|t| t.to_string());

        let tx = match self.conn.transaction_with_behavior(TransactionBehavior::Immediate) {
            Ok(t) => t,
            Err(_) => return,
        };

        let next_version: i64 = match tx.query_row(
            "SELECT COALESCE(MAX(version), -1) + 1 FROM events \
             WHERE stream_id = ?1 AND branch = ?2",
            rusqlite::params![SYSTEM_STREAM, SYSTEM_BRANCH],
            |r| r.get(0),
        ) {
            Ok(v) => v,
            Err(_) => return,
        };

        let _ = tx.execute(
            "INSERT OR IGNORE INTO events \
             (id, stream_id, branch, version, timestamp_us, event_type, type_version, payload, \
              indexed_tags) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)",
            rusqlite::params![
                event_id,
                SYSTEM_STREAM,
                SYSTEM_BRANCH,
                next_version,
                ts,
                event_type,
                payload_bytes,
                indexed_tags_json,
            ],
        );
        let _ = tx.commit();
    }

    /// Emit a `SubscriptionDegraded` event. Preserves the exact payload schema
    /// used before this abstraction was introduced — same field set, same CCE id.
    pub(crate) fn emit_subscription_degraded(
        &mut self,
        sub_id: u64,
        stream_id: &str,
        branch: &str,
        dropped_version: u64,
    ) {
        let payload = serde_json::json!({
            "subscription_id": sub_id,
            "stream_id": stream_id,
            "branch": branch,
            "dropped_version": dropped_version,
        });
        self.emit("SubscriptionDegraded", &payload, None);
    }
}
