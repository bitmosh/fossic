// SPDX-License-Identifier: Apache-2.0
use fossic::{Append, Error, OpenOptions, Store};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

fn decl(store: &Store, id: &str) {
    store.declare_stream(id, "test", None).unwrap();
}

fn ev(stream_id: &str) -> Append {
    Append {
        stream_id: stream_id.to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"x": 1}),
        ..Default::default()
    }
}

#[test]
fn append_if_true_condition_appends() {
    let (store, _dir) = open_tmp();
    decl(&store, "cond/a");

    let result = store.append_if(ev("cond/a"), |_conn| Ok(true)).unwrap();

    assert!(
        result.is_some(),
        "true condition must produce Some(event_id)"
    );
    let events = store
        .read_range(fossic::ReadQuery::stream("cond/a"))
        .unwrap();
    assert_eq!(events.len(), 1);
}

#[test]
fn append_if_false_condition_does_not_append() {
    let (store, _dir) = open_tmp();
    decl(&store, "cond/b");

    let result = store.append_if(ev("cond/b"), |_conn| Ok(false)).unwrap();

    assert!(result.is_none(), "false condition must produce None");
    let events = store
        .read_range(fossic::ReadQuery::stream("cond/b"))
        .unwrap();
    assert_eq!(
        events.len(),
        0,
        "no event must be written when condition is false"
    );
}

#[test]
fn append_if_false_condition_leaves_version_unchanged() {
    let (store, _dir) = open_tmp();
    decl(&store, "cond/ver");

    // Append one real event to establish version 0.
    store.append(ev("cond/ver")).unwrap();

    // Conditional append fails.
    store
        .append_if(
            Append {
                stream_id: "cond/ver".to_string(),
                event_type: "Other".to_string(),
                payload: serde_json::json!({"y": 2}),
                ..Default::default()
            },
            |_conn| Ok(false),
        )
        .unwrap();

    // Stream must still have exactly version 0.
    let events = store
        .read_range(fossic::ReadQuery::stream("cond/ver"))
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].version, 0);
}

#[test]
fn append_if_condition_sees_current_state() {
    let (store, _dir) = open_tmp();
    decl(&store, "cond/state");

    store.append(ev("cond/state")).unwrap(); // version 0
    store
        .append(Append {
            stream_id: "cond/state".to_string(),
            event_type: "Second".to_string(),
            payload: serde_json::json!({"n": 2}),
            ..Default::default()
        })
        .unwrap(); // version 1

    // Condition: only append if current max version == 1.
    let result = store
        .append_if(
            Append {
                stream_id: "cond/state".to_string(),
                event_type: "Third".to_string(),
                payload: serde_json::json!({"n": 3}),
                ..Default::default()
            },
            |conn| {
                let v: i64 = conn
                    .query_row(
                        "SELECT COALESCE(MAX(version), -1) FROM events \
                 WHERE stream_id = 'cond/state' AND branch = 'main'",
                        [],
                        |r| r.get(0),
                    )
                    .map_err(|e| Error::Internal(e.to_string()))?;
                Ok(v == 1)
            },
        )
        .unwrap();

    assert!(
        result.is_some(),
        "condition met: version==1 so append should succeed"
    );
    let events = store
        .read_range(fossic::ReadQuery::stream("cond/state"))
        .unwrap();
    assert_eq!(events.len(), 3);
}

#[test]
fn append_if_version_guard_rejects_stale_version() {
    let (store, _dir) = open_tmp();
    decl(&store, "cond/stale");

    store.append(ev("cond/stale")).unwrap(); // version 0

    // Caller thinks version is -1 (empty stream) — stale read.
    let result = store
        .append_if(
            Append {
                stream_id: "cond/stale".to_string(),
                event_type: "Stale".to_string(),
                payload: serde_json::json!({}),
                ..Default::default()
            },
            |conn| {
                let v: i64 = conn
                    .query_row(
                        "SELECT COALESCE(MAX(version), -1) FROM events \
                 WHERE stream_id = 'cond/stale' AND branch = 'main'",
                        [],
                        |r| r.get(0),
                    )
                    .map_err(|e| Error::Internal(e.to_string()))?;
                Ok(v == -1) // expects empty stream, but it has version 0
            },
        )
        .unwrap();

    assert!(
        result.is_none(),
        "stale version guard must reject the append"
    );
    let events = store
        .read_range(fossic::ReadQuery::stream("cond/stale"))
        .unwrap();
    assert_eq!(
        events.len(),
        1,
        "original event must remain, no new event written"
    );
}

#[test]
fn append_if_condition_error_propagates() {
    let (store, _dir) = open_tmp();
    decl(&store, "cond/err");

    let result = store.append_if(ev("cond/err"), |_conn| {
        Err(Error::Internal("deliberate condition error".into()))
    });

    assert!(result.is_err(), "condition error must propagate as Err");
}

#[test]
fn append_if_undeclared_stream_fails() {
    let (store, _dir) = open_tmp();

    let result = store.append_if(ev("no/such/stream"), |_conn| Ok(true));

    assert!(
        matches!(result, Err(Error::StreamNotDeclared { .. })),
        "undeclared stream must fail before condition runs"
    );
}
