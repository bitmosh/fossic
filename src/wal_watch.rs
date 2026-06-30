use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use notify::{RecursiveMode, Watcher};
use rusqlite::Connection;

use crate::{subscriptions::SubscriptionRegistry, types::StoredEvent};

// ── Public type ───────────────────────────────────────────────────────────────

/// Watches the WAL file of a SQLite store and fans new cross-process events
/// into the store's in-process dispatch channel.
///
/// Drop to stop watching.
pub(crate) struct WalWatcher {
    // Keeps the notify watcher alive. When dropped, the watcher's internal sender
    // closes, which disconnects the scan thread's receiver, causing it to exit.
    _watcher: notify::RecommendedWatcher,
}

impl WalWatcher {
    /// Start a WAL watcher for the store at `db_path`.
    ///
    /// Spawns a background scan thread. The thread exits when `WalWatcher` is dropped.
    pub fn start(
        db_path: PathBuf,
        dispatch_tx: crossbeam_channel::Sender<StoredEvent>,
        registry: Arc<SubscriptionRegistry>,
    ) -> Result<Self, String> {
        let (notify_tx, notify_rx) =
            crossbeam_channel::unbounded::<Result<notify::Event, notify::Error>>();

        let mut watcher = notify::RecommendedWatcher::new(
            move |res| {
                let _ = notify_tx.send(res);
            },
            notify::Config::default(),
        )
        .map_err(|e| format!("notify watcher init: {e}"))?;

        // Watch the parent directory so we catch WAL file creation and modification.
        let watch_dir = db_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        watcher
            .watch(&watch_dir, RecursiveMode::NonRecursive)
            .map_err(|e| format!("notify watch: {e}"))?;

        let db_path_clone = db_path.clone();
        std::thread::spawn(move || {
            run_scan_loop(db_path_clone, notify_rx, dispatch_tx, registry);
        });

        Ok(WalWatcher { _watcher: watcher })
    }
}

// ── Scan loop ─────────────────────────────────────────────────────────────────

fn wal_filename(db_path: &Path) -> std::ffi::OsString {
    let mut s = db_path
        .file_name()
        .unwrap_or(db_path.as_os_str())
        .to_owned();
    s.push("-wal");
    s
}

fn run_scan_loop(
    db_path: PathBuf,
    notify_rx: crossbeam_channel::Receiver<Result<notify::Event, notify::Error>>,
    dispatch_tx: crossbeam_channel::Sender<StoredEvent>,
    registry: Arc<SubscriptionRegistry>,
) {
    let conn = match Connection::open(&db_path) {
        Ok(c) => {
            let _ = c.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 30000;");
            c
        }
        Err(e) => {
            eprintln!("[WARN fossic] WAL watcher: failed to open read connection: {e}");
            return;
        }
    };

    let mut last_data_version: i64 = conn
        .query_row("PRAGMA data_version", [], |r| r.get(0))
        .unwrap_or(-1);

    let wal_name = wal_filename(&db_path);

    for result in &notify_rx {
        let event = match result {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[WARN fossic] WAL watcher notify error: {e}");
                continue;
            }
        };

        // Only process events affecting the WAL file.
        let relevant = event.paths.iter().any(|p| p.file_name() == Some(&wal_name));
        if !relevant {
            continue;
        }

        let current_version: i64 = match conn.query_row("PRAGMA data_version", [], |r| r.get(0)) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if current_version == last_data_version {
            // WAL was modified (checkpoint/truncation) but no new committed data.
            continue;
        }
        last_data_version = current_version;

        // Gather subscriptions to scan; group by (stream_id, branch) for efficiency.
        // Glob patterns are expanded to actual stream IDs from the DB; exact patterns
        // are used directly. The WAL watcher never advances subscription cursors —
        // dispatch_post_commit owns cursor advancement (CURSOR OWNERSHIP INVARIANT).
        let cursors = registry.post_commit_cursors();
        if cursors.is_empty() {
            continue;
        }

        let has_glob = cursors.iter().any(|(_, p, _, _)| p.contains('*'));
        let all_streams: Vec<String> = if has_glob {
            match list_all_streams(&conn) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[WARN fossic] WAL watcher stream enum error: {e}");
                    vec![]
                }
            }
        } else {
            vec![]
        };

        // Map (stream_id, branch) → min cursor across all subscribers.
        let mut group_min: HashMap<(String, String), i64> = HashMap::new();
        for (_, stream_pattern, branch, cursor) in &cursors {
            if stream_pattern.contains('*') {
                // Glob: expand to matching stream IDs. cursor = min(stream_cursors)
                // from post_commit_cursors; use it as the scan start for each stream
                // so already-delivered events are not re-fetched.
                for stream_id in all_streams
                    .iter()
                    .filter(|s| crate::glob::matches(stream_pattern, s))
                {
                    let key = (stream_id.clone(), branch.clone());
                    let entry = group_min.entry(key).or_insert(*cursor);
                    if *cursor < *entry {
                        *entry = *cursor;
                    }
                }
            } else {
                let key = (stream_pattern.clone(), branch.clone());
                let entry = group_min.entry(key).or_insert(*cursor);
                if *cursor < *entry {
                    *entry = *cursor;
                }
            }
        }

        for ((stream_id, branch), min_cursor) in &group_min {
            let new_events = match fetch_events_after(&conn, stream_id, branch, *min_cursor) {
                Ok(ev) => ev,
                Err(e) => {
                    eprintln!("[WARN fossic] WAL watcher scan error: {e}");
                    continue;
                }
            };

            for event in new_events {
                // Fan-out through the store dispatcher (same path as in-process writes).
                // dispatch_post_commit updates the WAL cursor after delivery for exact
                // subscriptions, preventing re-delivery on subsequent WAL notifications.
                if dispatch_tx.send(event).is_err() {
                    return; // Store shut down; exit thread.
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn list_all_streams(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM streams")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

fn fetch_events_after(
    conn: &Connection,
    stream_id: &str,
    branch: &str,
    after_version: i64,
) -> rusqlite::Result<Vec<StoredEvent>> {
    let sql = "SELECT id, stream_id, branch, version, timestamp_us, \
               causation_id, correlation_id, event_type, type_version, \
               payload, external_id, indexed_tags \
               FROM events \
               WHERE stream_id = ?1 AND branch = ?2 AND version > ?3 \
               ORDER BY version ASC";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![stream_id, branch, after_version], |row| {
        let indexed_tags_json: Option<String> = row.get(11)?;
        let indexed_tags = indexed_tags_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
        Ok(StoredEvent {
            id: row.get(0)?,
            stream_id: row.get(1)?,
            branch: row.get(2)?,
            version: row.get::<_, i64>(3)? as u64,
            timestamp_us: row.get(4)?,
            causation_id: row.get(5)?,
            correlation_id: row.get(6)?,
            event_type: row.get(7)?,
            type_version: row.get::<_, i64>(8)? as u32,
            payload: row.get(9)?,
            external_id: row.get(10)?,
            indexed_tags,
        })
    })?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}
