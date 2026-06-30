use fossic::{Append, OpenOptions, Store};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

fn with_stream(store: &Store, stream: &str) {
    store.declare_stream(stream, "test", None).ok();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn get_cursor_unset_returns_none() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "test/stream");
    let cursor = store
        .get_cursor("consumer-1", "test/stream", "main")
        .unwrap();
    assert!(cursor.is_none(), "unset cursor must return None");
}

#[test]
fn set_then_get_cursor() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "test/stream");
    store
        .set_cursor("consumer-1", "test/stream", "main", 42)
        .unwrap();
    let cursor = store
        .get_cursor("consumer-1", "test/stream", "main")
        .unwrap();
    assert_eq!(cursor, Some(42));
}

#[test]
fn cursor_update_overwrites_previous_value() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "test/stream");
    store
        .set_cursor("consumer-1", "test/stream", "main", 10)
        .unwrap();
    store
        .set_cursor("consumer-1", "test/stream", "main", 99)
        .unwrap();
    let cursor = store
        .get_cursor("consumer-1", "test/stream", "main")
        .unwrap();
    assert_eq!(cursor, Some(99));
}

#[test]
fn cursors_are_scoped_by_stream() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "stream/a");
    with_stream(&store, "stream/b");

    store
        .set_cursor("consumer-1", "stream/a", "main", 5)
        .unwrap();
    // stream/b cursor not set.
    let a = store.get_cursor("consumer-1", "stream/a", "main").unwrap();
    let b = store.get_cursor("consumer-1", "stream/b", "main").unwrap();
    assert_eq!(a, Some(5));
    assert!(b.is_none(), "cursor for stream/b must be independent");
}

#[test]
fn cursors_are_scoped_by_consumer() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "test/stream");

    store
        .set_cursor("consumer-A", "test/stream", "main", 100)
        .unwrap();
    store
        .set_cursor("consumer-B", "test/stream", "main", 200)
        .unwrap();

    let a = store
        .get_cursor("consumer-A", "test/stream", "main")
        .unwrap();
    let b = store
        .get_cursor("consumer-B", "test/stream", "main")
        .unwrap();
    assert_eq!(a, Some(100));
    assert_eq!(
        b,
        Some(200),
        "separate consumers must have independent cursors"
    );
}

#[test]
fn cursors_are_scoped_by_branch() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "test/stream");

    store
        .set_cursor("consumer-1", "test/stream", "main", 1)
        .unwrap();
    store
        .set_cursor("consumer-1", "test/stream", "experiment", 2)
        .unwrap();

    let main = store
        .get_cursor("consumer-1", "test/stream", "main")
        .unwrap();
    let exp = store
        .get_cursor("consumer-1", "test/stream", "experiment")
        .unwrap();
    assert_eq!(main, Some(1));
    assert_eq!(
        exp,
        Some(2),
        "cursors on different branches must be independent"
    );
}

#[test]
fn cursor_version_zero_is_valid() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "test/stream");
    store
        .set_cursor("consumer-1", "test/stream", "main", 0)
        .unwrap();
    let cursor = store
        .get_cursor("consumer-1", "test/stream", "main")
        .unwrap();
    assert_eq!(cursor, Some(0));
}

#[test]
fn cursor_can_be_set_without_prior_append() {
    // Cursors don't require events to exist in the stream.
    let (store, _dir) = open_tmp();
    store
        .set_cursor("consumer-1", "any/stream", "main", 7)
        .unwrap();
    let cursor = store
        .get_cursor("consumer-1", "any/stream", "main")
        .unwrap();
    assert_eq!(cursor, Some(7));
}

#[test]
fn cursor_tracks_append_progress() {
    let (store, _dir) = open_tmp();
    with_stream(&store, "test/events");

    // Simulate a consumer processing events and updating its cursor.
    for i in 0..5 {
        store
            .append(Append {
                stream_id: "test/events".to_string(),
                event_type: "E".to_string(),
                type_version: 1,
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
        store
            .set_cursor("consumer-1", "test/events", "main", i as u64)
            .unwrap();
    }

    let cursor = store
        .get_cursor("consumer-1", "test/events", "main")
        .unwrap();
    assert_eq!(cursor, Some(4));
}
