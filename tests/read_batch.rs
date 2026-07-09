// SPDX-License-Identifier: Apache-2.0
use fossic::{Append, EventId, OpenOptions, Store};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

fn append_ev(store: &Store, stream: &str, event_type: &str, n: u64) -> EventId {
    store
        .append(Append {
            stream_id: stream.to_string(),
            event_type: event_type.to_string(),
            payload: serde_json::json!({ "n": n }),
            ..Default::default()
        })
        .unwrap()
}

// ── Basic fetch ───────────────────────────────────────────────────────────────

#[test]
fn read_batch_empty_input() {
    let (store, _dir) = open_tmp();
    let result = store.read_batch(&[]).unwrap();
    assert!(result.is_empty());
}

#[test]
fn read_batch_single_id() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s", "test", None).unwrap();
    let id = append_ev(&store, "s", "Ev", 1);
    let result = store.read_batch(&[id]).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, id);
}

#[test]
fn read_batch_multiple_ids() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s", "test", None).unwrap();
    let id1 = append_ev(&store, "s", "Ev", 1);
    let id2 = append_ev(&store, "s", "Ev", 2);
    let id3 = append_ev(&store, "s", "Ev", 3);
    let result = store.read_batch(&[id1, id3, id2]).unwrap();
    assert_eq!(result.len(), 3);
    // results are timestamp-ordered, not input-ordered
    let ids: Vec<EventId> = result.iter().map(|e| e.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
    assert!(ids.contains(&id3));
}

#[test]
fn read_batch_timestamp_order() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s", "test", None).unwrap();
    let id1 = append_ev(&store, "s", "Ev", 1);
    let id2 = append_ev(&store, "s", "Ev", 2);
    let id3 = append_ev(&store, "s", "Ev", 3);
    // Pass in reverse order — result should be timestamp ASC.
    let result = store.read_batch(&[id3, id2, id1]).unwrap();
    assert_eq!(result[0].id, id1);
    assert_eq!(result[1].id, id2);
    assert_eq!(result[2].id, id3);
}

// ── Missing IDs ───────────────────────────────────────────────────────────────

#[test]
fn read_batch_missing_id_silently_omitted() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s", "test", None).unwrap();
    let present = append_ev(&store, "s", "Ev", 1);
    // Fabricate an ID that will never exist in the store.
    let absent = EventId::from_bytes([0xffu8; 32]);
    let result = store.read_batch(&[present, absent]).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, present);
}

#[test]
fn read_batch_all_missing_returns_empty() {
    let (store, _dir) = open_tmp();
    let absent = EventId::from_bytes([0xaau8; 32]);
    let result = store.read_batch(&[absent]).unwrap();
    assert!(result.is_empty());
}

// ── Cross-stream ──────────────────────────────────────────────────────────────

#[test]
fn read_batch_across_streams() {
    let (store, _dir) = open_tmp();
    store.declare_stream("a", "test", None).unwrap();
    store.declare_stream("b", "test", None).unwrap();
    let id_a = append_ev(&store, "a", "EvA", 1);
    let id_b = append_ev(&store, "b", "EvB", 2);
    let result = store.read_batch(&[id_a, id_b]).unwrap();
    assert_eq!(result.len(), 2);
    let stream_ids: Vec<&str> = result.iter().map(|e| e.stream_id.as_str()).collect();
    assert!(stream_ids.contains(&"a"));
    assert!(stream_ids.contains(&"b"));
}

// ── Deduplication ─────────────────────────────────────────────────────────────

#[test]
fn read_batch_duplicate_ids_returns_one_event() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s", "test", None).unwrap();
    let id = append_ev(&store, "s", "Ev", 1);
    // Same ID twice — SQLite deduplicates via IN clause.
    let result = store.read_batch(&[id, id]).unwrap();
    assert_eq!(result.len(), 1);
}

// ── Payload integrity ─────────────────────────────────────────────────────────

#[test]
fn read_batch_payload_matches_appended() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s", "test", None).unwrap();
    let id = store
        .append(Append {
            stream_id: "s".to_string(),
            event_type: "Rich".to_string(),
            payload: serde_json::json!({ "x": 42, "label": "hello" }),
            ..Default::default()
        })
        .unwrap();
    let result = store.read_batch(&[id]).unwrap();
    assert_eq!(result.len(), 1);
    let payload: serde_json::Value = rmp_serde::from_slice(&result[0].payload).unwrap();
    assert_eq!(payload["x"], 42);
    assert_eq!(payload["label"], "hello");
}
