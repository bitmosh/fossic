use fossic::{Append, EventId, OpenOptions, ReadQuery, Store};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

#[test]
fn open_and_close() {
    let (store, _dir) = open_tmp();
    store.close().unwrap();
}

#[test]
fn require_existing_fails_on_new_path() {
    let dir = tempfile::tempdir().unwrap();
    let result = Store::open(
        dir.path().join("nonexistent.db"),
        OpenOptions {
            on_first_open: fossic::FirstOpenPolicy::RequireExisting,
            ..Default::default()
        },
    );
    assert!(matches!(result, Err(fossic::Error::StoreNotFound { .. })));
}

#[test]
fn open_twice_same_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.db");
    let s1 = Store::open(&path, OpenOptions::default()).unwrap();
    let s2 = Store::open(&path, OpenOptions::default()).unwrap();
    s1.close().unwrap();
    s2.close().unwrap();
}

// ── Stream registry ───────────────────────────────────────────────────────────

#[test]
fn declare_stream_and_check() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("test/events/s1", "integration-test", None)
        .unwrap();
    assert!(store.stream_exists("test/events/s1").unwrap());
    assert!(!store.stream_exists("test/events/s2").unwrap());
}

#[test]
fn declare_stream_idempotent() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "a", Some("first")).unwrap();
    store.declare_stream("test/s", "b", Some("second")).unwrap(); // no error
    assert!(store.stream_exists("test/s").unwrap());
}

#[test]
fn streams_list() {
    let (store, _dir) = open_tmp();
    store.declare_stream("alpha/s", "t", None).unwrap();
    store.declare_stream("beta/s", "t", None).unwrap();
    let list = store.streams().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "alpha/s");
    assert_eq!(list[1].id, "beta/s");
}

#[test]
fn invalid_stream_id_rejected_at_declare() {
    let (store, _dir) = open_tmp();
    assert!(store.declare_stream("", "t", None).is_err());
    assert!(store.declare_stream("has space", "t", None).is_err());
    assert!(store.declare_stream("a/b/c/d/e", "t", None).is_err()); // 5 segments
    assert!(store.declare_stream("/leading", "t", None).is_err());
}

// ── Append ────────────────────────────────────────────────────────────────────

#[test]
fn append_to_undeclared_stream_fails() {
    let (store, _dir) = open_tmp();
    let result = store.append(Append {
        stream_id: "undeclared/s".to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"x": 1}),
        ..Default::default()
    });
    assert!(matches!(
        result,
        Err(fossic::Error::StreamNotDeclared { .. })
    ));
}

#[test]
fn append_returns_event_id() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    let id = store.append(Append {
        stream_id: "test/s".to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"key": "value"}),
        ..Default::default()
    });
    assert!(id.is_ok(), "{:?}", id);
}

#[test]
fn append_idempotent() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();

    let a = Append {
        stream_id: "test/s".to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"n": 42}),
        ..Default::default()
    };
    let id1 = store.append(Append { ..a }).unwrap();

    let a2 = Append {
        stream_id: "test/s".to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"n": 42}),
        ..Default::default()
    };
    let id2 = store.append(a2).unwrap();

    assert_eq!(id1, id2, "identical events must produce identical IDs");
}

#[test]
fn different_payloads_produce_different_ids() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    let id1 = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "E".to_string(),
            payload: serde_json::json!({"x": 1}),
            ..Default::default()
        })
        .unwrap();
    let id2 = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "E".to_string(),
            payload: serde_json::json!({"x": 2}),
            ..Default::default()
        })
        .unwrap();
    assert_ne!(id1, id2);
}

#[test]
fn causation_changes_event_id() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    let cause_id = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "Root".to_string(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();

    let no_cause = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "Child".to_string(),
            payload: serde_json::json!({"x": 1}),
            ..Default::default()
        })
        .unwrap();

    let with_cause = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "Child".to_string(),
            payload: serde_json::json!({"x": 1}),
            causation_id: Some(cause_id),
            ..Default::default()
        })
        .unwrap();

    assert_ne!(no_cause, with_cause);
}

#[test]
fn indexed_tags_must_be_object() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    // Array is rejected
    let r = store.append(Append {
        stream_id: "test/s".to_string(),
        event_type: "E".to_string(),
        payload: serde_json::json!({}),
        indexed_tags: Some(serde_json::json!([1, 2, 3])),
        ..Default::default()
    });
    assert!(matches!(r, Err(fossic::Error::InvalidIndexedTags { .. })));
    // Object is accepted
    let r2 = store.append(Append {
        stream_id: "test/s".to_string(),
        event_type: "E".to_string(),
        payload: serde_json::json!({}),
        indexed_tags: Some(serde_json::json!({"tag": "value"})),
        ..Default::default()
    });
    assert!(r2.is_ok(), "{:?}", r2);
}

#[test]
fn append_batch_single_transaction() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    let batch: Vec<Append> = (0..5)
        .map(|i| Append {
            stream_id: "test/s".to_string(),
            event_type: "BatchEvent".to_string(),
            payload: serde_json::json!({"i": i}),
            ..Default::default()
        })
        .collect();
    let ids = store.append_batch(&batch).unwrap();
    assert_eq!(ids.len(), 5);
    // All IDs must be distinct
    let unique: std::collections::HashSet<EventId> = ids.iter().cloned().collect();
    assert_eq!(unique.len(), 5);
}

// ── Read ──────────────────────────────────────────────────────────────────────

#[test]
fn read_range_returns_events_in_version_order() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..5u64 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "E".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let events = store.read_range(ReadQuery::stream("test/s")).unwrap();
    assert_eq!(events.len(), 5);
    for (i, ev) in events.iter().enumerate() {
        assert_eq!(ev.version, i as u64);
        assert_eq!(ev.event_type, "E");
    }
}

#[test]
fn read_range_with_from_version() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..10u64 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "E".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let events = store
        .read_range(ReadQuery {
            stream_id: "test/s".to_string(),
            branch: "main".to_string(),
            from_version: Some(5),
            to_version: None,
            limit: None,
            event_type_filter: None,
        })
        .unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(events[0].version, 5);
    assert_eq!(events[4].version, 9);
}

#[test]
fn read_range_with_limit() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..10u64 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "E".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let events = store
        .read_range(ReadQuery {
            stream_id: "test/s".to_string(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: Some(3),
            event_type_filter: None,
        })
        .unwrap();
    assert_eq!(events.len(), 3);
}

#[test]
fn read_one_by_event_id() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    let id = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "TargetEvent".to_string(),
            payload: serde_json::json!({"secret": "xyzzy"}),
            ..Default::default()
        })
        .unwrap();

    let found = store.read_one(id).unwrap();
    assert!(found.is_some());
    let ev = found.unwrap();
    assert_eq!(ev.event_type, "TargetEvent");

    let decoded: serde_json::Value = ev.deserialize_payload_json().unwrap();
    assert_eq!(decoded["secret"], "xyzzy");
}

#[test]
fn read_one_missing_returns_none() {
    let (store, _dir) = open_tmp();
    let fake_id = EventId::from_bytes([0x42u8; 32]);
    let result = store.read_one(fake_id).unwrap();
    assert!(result.is_none());
}

#[test]
fn read_by_external_id() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "E".to_string(),
            payload: serde_json::json!({"v": 1}),
            external_id: Some("ext-001".to_string()),
            ..Default::default()
        })
        .unwrap();

    let found = store.read_by_external_id("test/s", "ext-001").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().external_id.as_deref(), Some("ext-001"));

    let missing = store.read_by_external_id("test/s", "ext-999").unwrap();
    assert!(missing.is_none());
}

#[test]
fn payload_round_trip_json() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    let payload = serde_json::json!({
        "int": 42,
        "float": 1.5,
        "string": "hello",
        "bool": true,
        "null": null,
        "array": [1, 2, 3],
        "nested": {"a": 1}
    });
    let id = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "RichEvent".to_string(),
            payload: payload.clone(),
            ..Default::default()
        })
        .unwrap();

    let ev = store.read_one(id).unwrap().unwrap();
    let decoded: serde_json::Value = ev.deserialize_payload_json().unwrap();
    assert_eq!(decoded["int"], 42);
    assert_eq!(decoded["string"], "hello");
    assert_eq!(decoded["bool"], true);
    assert_eq!(decoded["null"], serde_json::Value::Null);
    assert_eq!(decoded["nested"]["a"], 1);
}

#[test]
fn indexed_tags_round_trip() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    let id = store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "E".to_string(),
            payload: serde_json::json!({}),
            indexed_tags: Some(serde_json::json!({"category": "audit", "priority": 1})),
            ..Default::default()
        })
        .unwrap();

    let ev = store.read_one(id).unwrap().unwrap();
    let tags = ev.indexed_tags.unwrap();
    assert_eq!(tags["category"], "audit");
    assert_eq!(tags["priority"], 1);
}

#[test]
fn version_monotonically_increases() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..10u64 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "E".to_string(),
                // distinct payload per iteration so IDs differ (no dedup)
                payload: serde_json::json!({"seq": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let events = store.read_range(ReadQuery::stream("test/s")).unwrap();
    let versions: Vec<u64> = events.iter().map(|e| e.version).collect();
    let mut sorted = versions.clone();
    sorted.sort();
    assert_eq!(
        versions, sorted,
        "versions must be monotonically increasing"
    );
    assert_eq!(versions[0], 0);
    assert_eq!(versions[9], 9);
}

// ── event_type_filter ─────────────────────────────────────────────────────────

#[test]
fn read_range_event_type_filter_returns_matching() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..3u32 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "Alpha".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "Beta".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let mut q = ReadQuery::stream("test/s");
    q.event_type_filter = Some("Alpha".to_string());
    let events = store.read_range(q).unwrap();
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.event_type == "Alpha"));
}

#[test]
fn read_range_event_type_filter_no_match_returns_empty() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store
        .append(Append {
            stream_id: "test/s".to_string(),
            event_type: "Alpha".to_string(),
            payload: serde_json::json!({"i": 0}),
            ..Default::default()
        })
        .unwrap();
    let mut q = ReadQuery::stream("test/s");
    q.event_type_filter = Some("NoSuchType".to_string());
    let events = store.read_range(q).unwrap();
    assert!(events.is_empty());
}

#[test]
fn read_range_event_type_filter_combined_with_from_version() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..5u32 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "Alpha".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    // versions 0–4; ask from_version=2 and filter Alpha
    let mut q = ReadQuery::stream("test/s");
    q.from_version = Some(2);
    q.event_type_filter = Some("Alpha".to_string());
    let events = store.read_range(q).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].version, 2);
}

#[test]
fn read_range_event_type_filter_limit_applied_after_filter() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..10u32 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "Alpha".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "Beta".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let mut q = ReadQuery::stream("test/s");
    q.event_type_filter = Some("Alpha".to_string());
    q.limit = Some(4);
    let events = store.read_range(q).unwrap();
    assert_eq!(events.len(), 4);
    assert!(events.iter().all(|e| e.event_type == "Alpha"));
}

#[test]
fn read_range_event_type_filter_none_returns_all() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    for i in 0..3u32 {
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "Alpha".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
        store
            .append(Append {
                stream_id: "test/s".to_string(),
                event_type: "Beta".to_string(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let q = ReadQuery::stream("test/s");
    // event_type_filter is None by default
    let events = store.read_range(q).unwrap();
    assert_eq!(events.len(), 6);
}

// ── EventId helpers ───────────────────────────────────────────────────────────

#[test]
fn event_id_hex_round_trip() {
    let id = EventId::from_bytes([0xABu8; 32]);
    let hex = id.to_hex();
    assert_eq!(hex.len(), 64);
    let id2 = EventId::from_hex(&hex).unwrap();
    assert_eq!(id, id2);
}

#[test]
fn event_id_from_hex_invalid() {
    assert!(EventId::from_hex("tooshort").is_err());
    assert!(EventId::from_hex(&"g".repeat(64)).is_err()); // invalid hex char
}
