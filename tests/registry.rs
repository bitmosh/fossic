// SPDX-License-Identifier: Apache-2.0
use fossic::{OpenOptions, ReadQuery, Store};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open");
    (store, dir)
}

fn decode(bytes: &[u8]) -> serde_json::Value {
    rmp_serde::from_slice(bytes).expect("msgpack decode")
}

#[test]
fn emit_project_registered_writes_system_event() {
    let (store, _dir) = open_tmp();
    store
        .emit_project_registered(
            "my-project",
            "/home/user/project/store.db",
            "my-project/**",
            "Test project",
        )
        .unwrap();

    let events = store
        .read_range(ReadQuery {
            stream_id: "_fossic/system".to_string(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
            event_type_filter: Some("ProjectRegistered".to_string()),
        })
        .unwrap();

    assert_eq!(events.len(), 1, "expected one ProjectRegistered event");
    let payload = decode(&events[0].payload);
    assert_eq!(payload["source_store"], "my-project");
    assert_eq!(payload["subscribe_pattern"], "my-project/**");
}

#[test]
fn emit_relay_heartbeat_writes_system_event() {
    let (store, _dir) = open_tmp();
    store
        .emit_relay_heartbeat("my-project", 42, 0, 1_000_000)
        .unwrap();

    let events = store
        .read_range(ReadQuery {
            stream_id: "_fossic/system".to_string(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
            event_type_filter: Some("RelayHeartbeat".to_string()),
        })
        .unwrap();

    assert_eq!(events.len(), 1, "expected one RelayHeartbeat event");
    let payload = decode(&events[0].payload);
    assert_eq!(payload["source_store"], "my-project");
    assert_eq!(payload["last_event_version"], 42);
    assert_eq!(payload["queue_lag"], 0);
}

#[test]
fn emit_project_registered_indexed_tag_source_store() {
    let (store, _dir) = open_tmp();
    store
        .emit_project_registered("cerebra", "/path/to/db", "cerebra/**", "")
        .unwrap();

    let events = store
        .read_range(ReadQuery {
            stream_id: "_fossic/system".to_string(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
            event_type_filter: Some("ProjectRegistered".to_string()),
        })
        .unwrap();

    assert_eq!(events.len(), 1);
    let tags = events[0]
        .indexed_tags
        .as_ref()
        .expect("indexed_tags must be Some");
    assert_eq!(
        tags.get("source_store").and_then(|v| v.as_str()),
        Some("cerebra"),
        "indexed_tags must carry source_store"
    );
}

#[test]
fn multiple_heartbeats_are_distinct_appends() {
    // Different uptime_us → different CCE hash → two separate events.
    let (store, _dir) = open_tmp();
    store.emit_relay_heartbeat("proj", 0, 0, 1_000).unwrap();
    store.emit_relay_heartbeat("proj", 1, 0, 2_000).unwrap();

    let events = store
        .read_range(ReadQuery {
            stream_id: "_fossic/system".to_string(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
            event_type_filter: Some("RelayHeartbeat".to_string()),
        })
        .unwrap();
    assert_eq!(events.len(), 2, "distinct heartbeats must each appear");
}
