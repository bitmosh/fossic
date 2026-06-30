use fossic::{Append, OpenOptions, ReadQuery, Store};

const PURGE_CONFIRM: &str = "I understand this breaks replay-from-zero";

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn append_one(store: &Store, stream: &str) -> fossic::EventId {
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    store.declare_stream(stream, "test", None).ok();
    store
        .append(Append {
            stream_id: stream.to_string(),
            event_type: "TestEvent".to_string(),
            type_version: 1,
            // Unique payload per call so CCE produces a distinct id every time.
            payload: serde_json::json!({"seq": seq}),
            ..Default::default()
        })
        .unwrap()
}

// ── purge_event ───────────────────────────────────────────────────────────────

#[test]
fn purge_wrong_confirm_returns_error() {
    let (store, _dir) = open_tmp();
    let id = append_one(&store, "test/stream");

    let result = store.purge_event(id, "wrong string", "test reason", "tester@example.com");
    assert!(
        matches!(result, Err(fossic::Error::PurgeConfirmationError { .. })),
        "wrong confirm must return PurgeConfirmationError"
    );
    // Original event must still exist.
    assert!(
        store.read_one(id).unwrap().is_some(),
        "original event must survive wrong confirm"
    );
}

#[test]
fn purge_correct_confirm_removes_original_event() {
    let (store, _dir) = open_tmp();
    let id = append_one(&store, "test/stream");

    store
        .purge_event(id, PURGE_CONFIRM, "test purge", "tester@example.com")
        .unwrap();

    let gone = store.read_one(id).unwrap();
    assert!(gone.is_none(), "original event must be gone after purge");
}

#[test]
fn purge_writes_audit_event_to_system_stream() {
    let (store, _dir) = open_tmp();
    let id = append_one(&store, "test/stream");

    store
        .purge_event(
            id,
            PURGE_CONFIRM,
            "audit trail test",
            "security@example.com",
        )
        .unwrap();

    let system_events = store
        .read_range(ReadQuery::stream("_fossic/system"))
        .unwrap();
    assert!(
        !system_events.is_empty(),
        "Purged audit event must be written to _fossic/system"
    );

    let purged_event = &system_events[0];
    assert_eq!(purged_event.event_type, "Purged");

    let payload: serde_json::Value = purged_event.deserialize_payload_json().unwrap();
    assert_eq!(payload["event_id_purged"].as_str().unwrap(), id.to_hex());
    assert_eq!(payload["original_event_type"], "TestEvent");
    assert_eq!(payload["original_stream_id"], "test/stream");
    assert_eq!(payload["reason"], "audit trail test");
    assert_eq!(payload["purged_by"], "security@example.com");
    // Original payload must NOT be in the Purged event.
    assert!(payload.get("data").is_none());
}

#[test]
fn purge_audit_event_written_before_deletion_atomically() {
    // Verify that after a successful purge: Purged event exists AND original is gone.
    // This tests the transaction guarantee (both happen atomically).
    let (store, _dir) = open_tmp();
    let id = append_one(&store, "test/stream");

    store
        .purge_event(id, PURGE_CONFIRM, "atomicity test", "ops@example.com")
        .unwrap();

    // Audit event exists.
    let system = store
        .read_range(ReadQuery::stream("_fossic/system"))
        .unwrap();
    assert_eq!(system.len(), 1, "exactly one Purged event must exist");

    // Original is gone.
    assert!(store.read_one(id).unwrap().is_none());
}

#[test]
fn purge_nonexistent_event_returns_error() {
    let (store, _dir) = open_tmp();
    let fake_id = fossic::EventId::from_bytes([0u8; 32]);

    let result = store.purge_event(fake_id, PURGE_CONFIRM, "test", "ops@example.com");
    assert!(
        matches!(result, Err(fossic::Error::EventNotFound { .. })),
        "purging a nonexistent event must return EventNotFound"
    );

    // No audit event should have been written.
    // _fossic/system may not even be declared yet — that's fine.
    let system = store
        .read_range(ReadQuery::stream("_fossic/system"))
        .unwrap_or_default();
    assert!(
        system.is_empty(),
        "no Purged event written on EventNotFound path"
    );
}

#[test]
fn purge_does_not_affect_other_events_in_stream() {
    let (store, _dir) = open_tmp();
    let id1 = append_one(&store, "test/stream");
    let id2 = append_one(&store, "test/stream");
    let id3 = append_one(&store, "test/stream");

    store
        .purge_event(id2, PURGE_CONFIRM, "purge middle", "ops@example.com")
        .unwrap();

    assert!(store.read_one(id1).unwrap().is_some(), "id1 must survive");
    assert!(store.read_one(id2).unwrap().is_none(), "id2 must be gone");
    assert!(store.read_one(id3).unwrap().is_some(), "id3 must survive");
}

#[test]
fn multiple_purges_each_write_audit_event() {
    let (store, _dir) = open_tmp();
    let id1 = append_one(&store, "test/stream");
    let id2 = append_one(&store, "test/stream");

    store
        .purge_event(id1, PURGE_CONFIRM, "purge 1", "ops@example.com")
        .unwrap();
    store
        .purge_event(id2, PURGE_CONFIRM, "purge 2", "ops@example.com")
        .unwrap();

    let system = store
        .read_range(ReadQuery::stream("_fossic/system"))
        .unwrap();
    assert_eq!(system.len(), 2, "two purges must produce two audit events");
}

// ── shred_stream ──────────────────────────────────────────────────────────────

#[test]
fn shred_stream_plaintext_mode_returns_not_implemented() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/stream", "test", None).unwrap();

    let result = store.shred_stream("test/stream", "user requested GDPR deletion");
    assert!(
        matches!(result, Err(fossic::Error::NotImplemented { .. })),
        "shred_stream in plaintext mode must return NotImplemented"
    );
}
