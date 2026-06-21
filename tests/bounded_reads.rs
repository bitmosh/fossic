use fossic::{Append, OpenOptions, ReadOutcome, ReadQuery, Store, TruncationReason};

fn temp_store() -> Store {
    let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    Store::open(path, OpenOptions::default()).unwrap()
}

fn temp_store_with_defaults(max_results: Option<usize>, max_bytes: Option<usize>) -> Store {
    let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    Store::open(
        path,
        OpenOptions {
            default_max_results: max_results,
            default_max_bytes: max_bytes,
            ..Default::default()
        },
    )
    .unwrap()
}

fn append_n(store: &Store, n: usize) {
    store.declare_stream("s", "main", None).unwrap();
    for i in 0..n {
        store
            .append(Append {
                stream_id: "s".into(),
                branch: "main".into(),
                event_type: "Evt".into(),
                payload: serde_json::json!({ "i": i }),
                ..Default::default()
            })
            .unwrap();
    }
}

fn range_q() -> ReadQuery {
    ReadQuery {
        stream_id: "s".into(),
        branch: "main".into(),
        from_version: None,
        to_version: None,
        limit: None,
        event_type_filter: None,
    }
}

// ── read_range_bounded ────────────────────────────────────────────────────────

#[test]
fn range_bounded_no_budget_returns_complete() {
    let store = temp_store();
    append_n(&store, 5);
    let outcome = store
        .read_range_bounded(range_q(), None, None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 5),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn range_bounded_truncates_at_result_count() {
    let store = temp_store();
    append_n(&store, 10);
    let outcome = store
        .read_range_bounded(range_q(), Some(3), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 3);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn range_bounded_complete_when_exactly_at_limit() {
    let store = temp_store();
    append_n(&store, 5);
    let outcome = store
        .read_range_bounded(range_q(), Some(5), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 5),
        ReadOutcome::Truncated { .. } => panic!("expected Complete — limit == event count"),
    }
}

#[test]
fn range_bounded_truncates_at_byte_budget() {
    let store = temp_store();
    append_n(&store, 10);
    // Each payload serialises to a small msgpack blob. Budget of 1 byte will
    // only include the first event (at-least-one guarantee), then stop.
    let outcome = store
        .read_range_bounded(range_q(), None, Some(1), None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 1);
            assert_eq!(reason, TruncationReason::ByteSize);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn range_bounded_resume_continues_from_cursor() {
    let store = temp_store();
    append_n(&store, 6);

    // Page 1: take 3
    let (cursor, page1_versions) = match store
        .read_range_bounded(range_q(), Some(3), None, None)
        .unwrap()
    {
        ReadOutcome::Truncated { data, cursor, .. } => {
            let vs: Vec<u64> = data.iter().map(|e| e.version).collect();
            (cursor, vs)
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated on page 1"),
    };
    assert_eq!(page1_versions, vec![0, 1, 2]);

    // Page 2: resume
    let page2 = match store
        .read_range_bounded(range_q(), Some(3), None, cursor)
        .unwrap()
    {
        ReadOutcome::Complete(events) => events,
        ReadOutcome::Truncated { data, .. } => data,
    };
    let page2_versions: Vec<u64> = page2.iter().map(|e| e.version).collect();
    assert_eq!(page2_versions, vec![3, 4, 5]);
}

#[test]
fn range_bounded_resume_full_pagination() {
    let store = temp_store();
    append_n(&store, 7);

    let mut all_versions: Vec<u64> = Vec::new();
    let mut cursor_opt = None;
    loop {
        match store
            .read_range_bounded(range_q(), Some(3), None, cursor_opt)
            .unwrap()
        {
            ReadOutcome::Complete(events) => {
                all_versions.extend(events.iter().map(|e| e.version));
                break;
            }
            ReadOutcome::Truncated { data, cursor, .. } => {
                all_versions.extend(data.iter().map(|e| e.version));
                cursor_opt = cursor;
            }
        }
    }
    assert_eq!(all_versions, vec![0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn range_bounded_uses_store_default_max_results() {
    let store = temp_store_with_defaults(Some(2), None);
    append_n(&store, 5);
    let outcome = store
        .read_range_bounded(range_q(), None, None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 2);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated via store default"),
    }
}

#[test]
fn range_bounded_per_call_overrides_store_default() {
    let store = temp_store_with_defaults(Some(2), None);
    append_n(&store, 5);
    // Per-call limit of 4 overrides the store default of 2
    let outcome = store
        .read_range_bounded(range_q(), Some(4), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, .. } => assert_eq!(data.len(), 4),
        ReadOutcome::Complete(_) => panic!("expected Truncated with per-call limit"),
    }
}

// ── read_by_correlation_bounded ───────────────────────────────────────────────

fn append_correlated(store: &Store, n: usize) -> fossic::EventId {
    store.declare_stream("corr", "main", None).unwrap();
    // Append root event; all subsequent events share its id as correlation_id.
    let root_id = store
        .append(Append {
            stream_id: "corr".into(),
            branch: "main".into(),
            event_type: "Root".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();

    for i in 0..n {
        store
            .append(Append {
                stream_id: "corr".into(),
                branch: "main".into(),
                event_type: "Child".into(),
                payload: serde_json::json!({ "i": i }),
                correlation_id: Some(root_id),
                ..Default::default()
            })
            .unwrap();
    }
    root_id
}

#[test]
fn correlation_bounded_no_budget_returns_complete() {
    let store = temp_store();
    let root = append_correlated(&store, 4);
    let outcome = store
        .read_by_correlation_bounded(root, None, None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 4),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn correlation_bounded_truncates_at_result_count() {
    let store = temp_store();
    let root = append_correlated(&store, 6);
    let outcome = store
        .read_by_correlation_bounded(root, Some(3), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 3);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn correlation_bounded_resume_continues_from_cursor() {
    let store = temp_store();
    let root = append_correlated(&store, 6);

    let mut all_ids: Vec<[u8; 32]> = Vec::new();
    let mut cursor_opt = None;
    loop {
        match store
            .read_by_correlation_bounded(root, Some(3), None, cursor_opt)
            .unwrap()
        {
            ReadOutcome::Complete(events) => {
                all_ids.extend(events.iter().map(|e| *e.id.as_bytes()));
                break;
            }
            ReadOutcome::Truncated { data, cursor, .. } => {
                all_ids.extend(data.iter().map(|e| *e.id.as_bytes()));
                cursor_opt = cursor;
            }
        }
    }
    assert_eq!(all_ids.len(), 6);
    // Ids must be strictly ascending (BLOB lexicographic ORDER BY id ASC)
    for w in all_ids.windows(2) {
        assert!(w[0] < w[1], "ids not in ascending order");
    }
}

#[test]
fn correlation_bounded_no_events_returns_complete_empty() {
    let store = temp_store();
    store.declare_stream("empty_corr", "main", None).unwrap();
    // Use a random EventId that no event carries as correlation_id.
    let fake_root = store
        .append(Append {
            stream_id: "empty_corr".into(),
            branch: "main".into(),
            event_type: "Lone".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let outcome = store
        .read_by_correlation_bounded(fake_root, Some(10), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 0),
        ReadOutcome::Truncated { .. } => panic!("expected Complete(empty)"),
    }
}

#[test]
fn correlation_bounded_uses_store_default_max_results() {
    let store = temp_store_with_defaults(Some(2), None);
    let root = append_correlated(&store, 5);
    let outcome = store
        .read_by_correlation_bounded(root, None, None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 2);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated via store default"),
    }
}

#[test]
fn correlation_bounded_wrong_cursor_type_returns_error() {
    let store = temp_store();
    append_n(&store, 3);          // populates stream "s" for range_q()
    let root = append_correlated(&store, 3);
    // Build a Range cursor and pass it to a correlation query — should error.
    let range_cursor = match store
        .read_range_bounded(range_q(), Some(1), None, None)
        .unwrap()
    {
        ReadOutcome::Truncated { cursor, .. } => cursor,
        ReadOutcome::Complete(_) => panic!("need truncated to get a cursor"),
    };
    let result = store.read_by_correlation_bounded(root, Some(10), None, range_cursor);
    assert!(result.is_err(), "mismatched cursor type should return Err");
}
