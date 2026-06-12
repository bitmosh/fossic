use fossic::{Append, OpenOptions, Store, SubscribeQuery, SubscriptionHandler, SubscriptionMode};
use fossic::StoredEvent;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn open_at(path: &std::path::Path) -> Store {
    Store::open(path, OpenOptions::default()).expect("open store")
}

fn decl(store: &Store, id: &str) {
    store.declare_stream(id, "wal-test", None).unwrap();
}

fn ev(stream_id: &str) -> Append {
    Append {
        stream_id: stream_id.to_string(),
        event_type: "WalTestEvent".to_string(),
        payload: serde_json::json!({"v": 42}),
        ..Default::default()
    }
}

fn unique_ev(stream_id: &str) -> Append {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    Append {
        stream_id: stream_id.to_string(),
        event_type: "WalTestEvent".to_string(),
        payload: serde_json::json!({"seq": seq}),
        ..Default::default()
    }
}

fn wait_for<F: Fn() -> bool>(f: F, timeout_ms: u64) -> bool {
    let start = std::time::Instant::now();
    loop {
        if f() {
            return true;
        }
        if start.elapsed().as_millis() > timeout_ms as u128 {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

struct FnHandler<F: Fn(&StoredEvent) + Send + Sync + 'static>(F);
impl<F: Fn(&StoredEvent) + Send + Sync + 'static> SubscriptionHandler for FnHandler<F> {
    fn on_event(&self, event: &StoredEvent) {
        (self.0)(event)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Two Store instances on the same file. Store B subscribes; Store A writes.
/// The WAL watcher on Store B should detect the write and deliver the event.
#[test]
fn cross_store_delivery_via_wal_watch() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("shared.db");

    let store_a = open_at(&db_path);
    let store_b = open_at(&db_path);

    decl(&store_a, "wal/events");

    let received = Arc::new(AtomicBool::new(false));
    let received2 = Arc::clone(&received);

    let _handle = store_b
        .subscribe(
            SubscribeQuery::stream("wal/events"),
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |_| received2.store(true, Ordering::Release)),
        )
        .unwrap();

    // Brief pause to let notify watchers register.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Write via store_a — not visible to store_b's in-process dispatch,
    // but visible via WAL watch.
    store_a.append(ev("wal/events")).unwrap();

    assert!(
        wait_for(|| received.load(Ordering::Acquire), 3000),
        "cross-store event must be delivered via WAL watcher within 3 seconds"
    );
}

/// WAL cursor prevents re-delivery of historical events written before subscription.
#[test]
fn wal_cursor_skips_historical_events() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("cursor.db");

    let store_a = open_at(&db_path);
    decl(&store_a, "wal/cursor");

    // Write 5 historical events before store_b subscribes (unique_ev so each is inserted).
    for _ in 0..5 {
        store_a.append(unique_ev("wal/cursor")).unwrap();
    }

    let store_b = open_at(&db_path);
    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    // Subscribe — cursor should be set to version 4 (the max at this moment).
    let _handle = store_b
        .subscribe(
            SubscribeQuery::stream("wal/cursor"),
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Write 3 new events via store_a after subscription (unique_ev for distinct CCE hashes).
    for _ in 0..3 {
        store_a.append(unique_ev("wal/cursor")).unwrap();
    }

    // Exactly 3 new events should arrive; the 5 historical ones must be skipped.
    assert!(
        wait_for(|| count.load(Ordering::Relaxed) == 3, 3000),
        "expected 3 new events via WAL watch, got {}",
        count.load(Ordering::Relaxed)
    );
}

/// Non-WAL file events in the same directory should not cause spurious dispatch.
#[test]
fn non_wal_file_events_no_spurious_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spurious.db");

    let store = open_at(&db_path);
    decl(&store, "wal/spurious");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("wal/spurious"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    // Create and write to an unrelated file in the same directory.
    let other_file = dir.path().join("other.txt");
    std::fs::write(&other_file, b"hello").unwrap();
    std::fs::write(&other_file, b"world").unwrap();

    std::thread::sleep(std::time::Duration::from_millis(200));

    assert_eq!(
        count.load(Ordering::Relaxed),
        0,
        "unrelated file events must not trigger event dispatch"
    );
}

/// Verify PRAGMA data_version semantics: reading it on an open connection sees
/// increments from writes on other connections.
#[test]
fn data_version_changes_on_cross_connection_write() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("dv.db");

    let store_a = open_at(&db_path);
    decl(&store_a, "dv/events");

    // Open a read connection to observe data_version.
    let read_conn = rusqlite::Connection::open(&db_path).unwrap();
    let v0: i64 = read_conn
        .query_row("PRAGMA data_version", [], |r| r.get(0))
        .unwrap();

    store_a.append(ev("dv/events")).unwrap();

    let v1: i64 = read_conn
        .query_row("PRAGMA data_version", [], |r| r.get(0))
        .unwrap();

    assert!(
        v1 > v0,
        "PRAGMA data_version must increment after a cross-connection write (v0={v0}, v1={v1})"
    );
}
