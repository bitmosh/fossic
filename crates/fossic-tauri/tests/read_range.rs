#![cfg(feature = "test-helpers")]

use fossic::{Append, OpenOptions, Store};
use tauri::Manager;

fn open_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("test.db"), OpenOptions::default()).unwrap();
    (store, dir)
}

fn make_app(store: Store) -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .plugin(fossic_tauri::plugin_with_test_helpers(store))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap()
}

#[test]
fn event_type_filter_returns_matching() {
    let (store, _dir) = open_store();
    store.declare_stream("test/s", "test", None).unwrap();
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

    let app = make_app(store);
    let state = app.state::<Store>();

    let result = fossic_tauri::commands::fossic_read_range(
        state,
        "test/s".to_string(),
        None,
        None,
        None,
        None,
        Some("Alpha".to_string()),
    )
    .map_err(|e| e.message)
    .expect("read_range with event_type_filter should succeed");

    assert_eq!(result.len(), 3);
    assert!(result.iter().all(|e| e.event_type == "Alpha"));
}

#[test]
fn event_type_filter_none_returns_all() {
    let (store, _dir) = open_store();
    store.declare_stream("test/s", "test", None).unwrap();
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

    let app = make_app(store);
    let state = app.state::<Store>();

    let result = fossic_tauri::commands::fossic_read_range(
        state,
        "test/s".to_string(),
        None,
        None,
        None,
        None,
        None,
    )
    .map_err(|e| e.message)
    .expect("read_range without filter should succeed");

    assert_eq!(result.len(), 6);
}

// ── Bounded read tests ────────────────────────────────────────────────────────

#[test]
fn read_range_bounded_no_cursor_returns_complete() {
    let (store, _dir) = open_store();
    store.declare_stream("test/b", "test", None).unwrap();
    for i in 0..5u32 {
        store
            .append(Append {
                stream_id: "test/b".into(),
                event_type: "E".into(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();
    let outcome = fossic_tauri::commands::fossic_read_range_bounded(
        state,
        "test/b".into(),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(outcome.kind, "complete");
    assert_eq!(outcome.results.len(), 5);
    assert!(outcome.next_cursor.is_none());
    assert!(outcome.reason.is_none());
}

#[test]
fn read_range_bounded_truncates_at_max_results() {
    let (store, _dir) = open_store();
    store.declare_stream("test/b2", "test", None).unwrap();
    for i in 0..10u32 {
        store
            .append(Append {
                stream_id: "test/b2".into(),
                event_type: "E".into(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();
    let outcome = fossic_tauri::commands::fossic_read_range_bounded(
        state,
        "test/b2".into(),
        None,
        None,
        None,
        None,
        Some(3),
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(outcome.kind, "truncated");
    assert_eq!(outcome.results.len(), 3);
    assert_eq!(outcome.reason.unwrap(), "result_count");
    assert!(outcome.next_cursor.is_some());
}

#[test]
fn read_range_from_cursor_resumes_correctly() {
    let (store, _dir) = open_store();
    store.declare_stream("test/b3", "test", None).unwrap();
    for i in 0..6u32 {
        store
            .append(Append {
                stream_id: "test/b3".into(),
                event_type: "E".into(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();

    let page1 = fossic_tauri::commands::fossic_read_range_bounded(
        state.clone(),
        "test/b3".into(),
        None,
        None,
        None,
        None,
        Some(3),
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(page1.kind, "truncated");
    let cursor = page1.next_cursor.clone().unwrap();

    let page2 = fossic_tauri::commands::fossic_read_range_from_cursor(
        state,
        "test/b3".into(),
        None,
        None,
        None,
        None,
        Some(3),
        None,
        cursor,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(page2.results.len(), 3);
    let v1: Vec<u64> = page1.results.iter().map(|e| e.version).collect();
    let v2: Vec<u64> = page2.results.iter().map(|e| e.version).collect();
    assert_eq!(v1, vec![0, 1, 2]);
    assert_eq!(v2, vec![3, 4, 5]);
}

#[test]
fn read_range_bounded_full_pagination() {
    let (store, _dir) = open_store();
    store.declare_stream("test/b4", "test", None).unwrap();
    for i in 0..7u32 {
        store
            .append(Append {
                stream_id: "test/b4".into(),
                event_type: "E".into(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();

    let mut all_versions: Vec<u64> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let outcome = if let Some(c) = cursor {
            fossic_tauri::commands::fossic_read_range_from_cursor(
                state.clone(),
                "test/b4".into(),
                None,
                None,
                None,
                None,
                Some(3),
                None,
                c,
            )
            .map_err(|e| e.message)
            .unwrap()
        } else {
            fossic_tauri::commands::fossic_read_range_bounded(
                state.clone(),
                "test/b4".into(),
                None,
                None,
                None,
                None,
                Some(3),
                None,
            )
            .map_err(|e| e.message)
            .unwrap()
        };
        all_versions.extend(outcome.results.iter().map(|e| e.version));
        if outcome.kind == "complete" {
            break;
        }
        cursor = outcome.next_cursor;
    }
    assert_eq!(all_versions, vec![0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn read_by_correlation_bounded_paginates() {
    let (store, _dir) = open_store();
    store.declare_stream("test/c", "test", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "test/c".into(),
            event_type: "Root".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    for i in 0..5u32 {
        store
            .append(Append {
                stream_id: "test/c".into(),
                event_type: "Child".into(),
                payload: serde_json::json!({"i": i}),
                correlation_id: Some(root),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();
    let outcome = fossic_tauri::commands::fossic_read_by_correlation_bounded(
        state,
        root.to_hex(),
        Some(3),
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(outcome.kind, "truncated");
    assert_eq!(outcome.results.len(), 3);
    assert!(outcome.next_cursor.is_some());
}

#[test]
fn read_by_correlation_from_cursor_resumes() {
    let (store, _dir) = open_store();
    store.declare_stream("test/c2", "test", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "test/c2".into(),
            event_type: "Root".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    for i in 0..6u32 {
        store
            .append(Append {
                stream_id: "test/c2".into(),
                event_type: "Child".into(),
                payload: serde_json::json!({"i": i}),
                correlation_id: Some(root),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();

    let page1 = fossic_tauri::commands::fossic_read_by_correlation_bounded(
        state.clone(),
        root.to_hex(),
        Some(3),
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(page1.kind, "truncated");
    let cursor = page1.next_cursor.clone().unwrap();

    let page2 = fossic_tauri::commands::fossic_read_by_correlation_from_cursor(
        state,
        root.to_hex(),
        Some(3),
        None,
        cursor,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(page2.results.len(), 3);
    // ids from both pages must be disjoint
    let ids1: std::collections::HashSet<&str> =
        page1.results.iter().map(|e| e.id.as_str()).collect();
    let ids2: std::collections::HashSet<&str> =
        page2.results.iter().map(|e| e.id.as_str()).collect();
    assert!(ids1.is_disjoint(&ids2));
}

#[test]
fn cursor_base64_round_trip() {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use fossic::TruncationCursor;
    let bytes = vec![1u8, 2, 3, 255];
    let cursor = TruncationCursor::from_bytes(bytes.clone());
    let encoded = B64.encode(cursor.as_bytes());
    let decoded = B64.decode(&encoded).unwrap();
    assert_eq!(decoded, bytes);
}

#[test]
fn walk_causation_bounded_forward() {
    let (store, _dir) = open_store();
    store.declare_stream("test/chain", "test", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "test/chain".into(),
            event_type: "N".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let mut prev = root;
    for i in 0..4u32 {
        let id = store
            .append(Append {
                stream_id: "test/chain".into(),
                event_type: "N".into(),
                payload: serde_json::json!({"i": i}),
                causation_id: Some(prev),
                ..Default::default()
            })
            .unwrap();
        prev = id;
    }
    let app = make_app(store);
    let state = app.state::<Store>();
    let outcome = fossic_tauri::commands::fossic_walk_causation_bounded(
        state,
        root.to_hex(),
        "forward".into(),
        None,
        None,
        None,
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(outcome.kind, "complete");
    assert_eq!(outcome.results.len(), 4);
}

#[test]
fn walk_causation_bounded_truncates_at_max_results() {
    let (store, _dir) = open_store();
    store.declare_stream("test/chain2", "test", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "test/chain2".into(),
            event_type: "N".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let mut prev = root;
    for i in 0..6u32 {
        let id = store
            .append(Append {
                stream_id: "test/chain2".into(),
                event_type: "N".into(),
                payload: serde_json::json!({"i": i}),
                causation_id: Some(prev),
                ..Default::default()
            })
            .unwrap();
        prev = id;
    }
    let app = make_app(store);
    let state = app.state::<Store>();
    let outcome = fossic_tauri::commands::fossic_walk_causation_bounded(
        state,
        root.to_hex(),
        "forward".into(),
        None,
        None,
        Some(3),
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(outcome.kind, "truncated");
    assert_eq!(outcome.results.len(), 3);
    assert!(outcome.next_cursor.is_some());
}

#[test]
fn aggregate_bounded_cross_stream() {
    // Payloads include "s" to distinguish events across streams — the CCE identity
    // hash excludes stream_id, so identical payloads on two streams produce the same
    // event_id and the second INSERT is silently ignored.
    let (store, _dir) = open_store();
    store.declare_stream("agg/a", "test", None).unwrap();
    store.declare_stream("agg/b", "test", None).unwrap();
    for i in 0..3u32 {
        store
            .append(Append {
                stream_id: "agg/a".into(),
                event_type: "E".into(),
                payload: serde_json::json!({"i": i, "s": "a"}),
                ..Default::default()
            })
            .unwrap();
        store
            .append(Append {
                stream_id: "agg/b".into(),
                event_type: "E".into(),
                payload: serde_json::json!({"i": i, "s": "b"}),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();
    let outcome = fossic_tauri::commands::fossic_aggregate_bounded(
        state,
        "agg/*".into(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(outcome.kind, "complete");
    assert_eq!(outcome.results.len(), 6);
}

#[test]
fn aggregate_bounded_truncates_at_max_events() {
    let (store, _dir) = open_store();
    store.declare_stream("agg2/a", "test", None).unwrap();
    for i in 0..10u32 {
        store
            .append(Append {
                stream_id: "agg2/a".into(),
                event_type: "E".into(),
                payload: serde_json::json!({"i": i}),
                ..Default::default()
            })
            .unwrap();
    }
    let app = make_app(store);
    let state = app.state::<Store>();
    let outcome = fossic_tauri::commands::fossic_aggregate_bounded(
        state,
        "agg2/*".into(),
        None,
        None,
        None,
        None,
        None,
        Some(4),
        None,
    )
    .map_err(|e| e.message)
    .unwrap();
    assert_eq!(outcome.kind, "truncated");
    assert_eq!(outcome.results.len(), 4);
    assert_eq!(outcome.reason.unwrap(), "result_count");
}
