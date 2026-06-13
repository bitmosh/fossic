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
        store.append(Append { stream_id: "test/s".to_string(), event_type: "Alpha".to_string(), payload: serde_json::json!({"i": i}), ..Default::default() }).unwrap();
        store.append(Append { stream_id: "test/s".to_string(), event_type: "Beta".to_string(), payload: serde_json::json!({"i": i}), ..Default::default() }).unwrap();
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
    ).map_err(|e| e.message).expect("read_range with event_type_filter should succeed");

    assert_eq!(result.len(), 3);
    assert!(result.iter().all(|e| e.event_type == "Alpha"));
}

#[test]
fn event_type_filter_none_returns_all() {
    let (store, _dir) = open_store();
    store.declare_stream("test/s", "test", None).unwrap();
    for i in 0..3u32 {
        store.append(Append { stream_id: "test/s".to_string(), event_type: "Alpha".to_string(), payload: serde_json::json!({"i": i}), ..Default::default() }).unwrap();
        store.append(Append { stream_id: "test/s".to_string(), event_type: "Beta".to_string(), payload: serde_json::json!({"i": i}), ..Default::default() }).unwrap();
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
    ).map_err(|e| e.message).expect("read_range without filter should succeed");

    assert_eq!(result.len(), 6);
}
