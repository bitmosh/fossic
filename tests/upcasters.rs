// SPDX-License-Identifier: Apache-2.0
use fossic::{Append, Error, OpenOptions, Store, Upcaster};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

// ── Upcaster helpers ──────────────────────────────────────────────────────────

/// v1 payload has "score: int"; v2 splits it to "raw_score: int, normalized_score: float".
struct ScoreV1toV2;

impl Upcaster for ScoreV1toV2 {
    fn upcast(&self, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut val: serde_json::Value = rmp_serde::from_slice(payload).unwrap();
        if let Some(obj) = val.as_object_mut() {
            if let Some(score) = obj.remove("score").and_then(|v| v.as_i64()) {
                obj.insert("raw_score".to_string(), serde_json::json!(score));
                obj.insert(
                    "normalized_score".to_string(),
                    serde_json::json!(score as f64 / 100.0),
                );
            }
        }
        Ok(rmp_serde::to_vec(&val).unwrap())
    }
}

/// v2 payload adds "label: String" derived from raw_score.
struct ScoreV2toV3;

impl Upcaster for ScoreV2toV3 {
    fn upcast(&self, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut val: serde_json::Value = rmp_serde::from_slice(payload).unwrap();
        if let Some(obj) = val.as_object_mut() {
            let label = match obj.get("raw_score").and_then(|v| v.as_i64()) {
                Some(s) if s >= 80 => "high",
                Some(s) if s >= 50 => "medium",
                _ => "low",
            };
            obj.insert("label".to_string(), serde_json::json!(label));
        }
        Ok(rmp_serde::to_vec(&val).unwrap())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn upcaster_v1_to_v2_applied_at_read_time() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/memory/s1", "test", None)
        .unwrap();

    // Store a v1 event.
    let id = store
        .append(Append {
            stream_id: "cerebra/memory/s1".to_string(),
            event_type: "MemoryScored".to_string(),
            type_version: 1,
            payload: serde_json::json!({"content": "hello", "score": 75}),
            ..Default::default()
        })
        .unwrap();

    // Register upcaster and read back.
    store
        .register_upcaster("MemoryScored", 1, 2, ScoreV1toV2)
        .unwrap();

    let event = store.read_one(id).unwrap().unwrap();
    // type_version stays at 1 (original stored value).
    assert_eq!(event.type_version, 1, "stored type_version must not change");
    // But payload reflects v2 shape.
    let val: serde_json::Value = event.deserialize_payload_json().unwrap();
    assert!(val.get("score").is_none(), "score field must be removed");
    assert_eq!(val["raw_score"], 75);
    assert!((val["normalized_score"].as_f64().unwrap() - 0.75).abs() < 1e-9);
}

#[test]
fn chained_upcasters_v1_to_v3() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/memory/s1", "test", None)
        .unwrap();

    let id = store
        .append(Append {
            stream_id: "cerebra/memory/s1".to_string(),
            event_type: "MemoryScored".to_string(),
            type_version: 1,
            payload: serde_json::json!({"content": "x", "score": 90}),
            ..Default::default()
        })
        .unwrap();

    store
        .register_upcaster("MemoryScored", 1, 2, ScoreV1toV2)
        .unwrap();
    store
        .register_upcaster("MemoryScored", 2, 3, ScoreV2toV3)
        .unwrap();

    let event = store.read_one(id).unwrap().unwrap();
    assert_eq!(event.type_version, 1, "stored type_version unchanged");
    let val: serde_json::Value = event.deserialize_payload_json().unwrap();
    // v1→v2: score→raw_score + normalized_score
    assert_eq!(val["raw_score"], 90);
    // v2→v3: label added
    assert_eq!(val["label"], "high");
}

#[test]
fn read_range_applies_upcaster() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/memory/s2", "test", None)
        .unwrap();

    for i in 0..3 {
        store
            .append(Append {
                stream_id: "cerebra/memory/s2".to_string(),
                event_type: "MemoryScored".to_string(),
                type_version: 1,
                payload: serde_json::json!({"score": i * 10}),
                ..Default::default()
            })
            .unwrap();
    }

    store
        .register_upcaster("MemoryScored", 1, 2, ScoreV1toV2)
        .unwrap();

    let events = store
        .read_range(fossic::ReadQuery::stream("cerebra/memory/s2"))
        .unwrap();
    assert_eq!(events.len(), 3);
    for e in &events {
        let val: serde_json::Value = e.deserialize_payload_json().unwrap();
        assert!(val.get("score").is_none(), "score removed in all events");
        assert!(
            val.get("raw_score").is_some(),
            "raw_score present in all events"
        );
    }
}

#[test]
fn no_upcaster_passthrough() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/stream", "test", None).unwrap();

    let id = store
        .append(Append {
            stream_id: "test/stream".to_string(),
            event_type: "PlainEvent".to_string(),
            type_version: 1,
            payload: serde_json::json!({"key": "value"}),
            ..Default::default()
        })
        .unwrap();

    // No upcaster registered — original payload returned unchanged.
    let event = store.read_one(id).unwrap().unwrap();
    let val: serde_json::Value = event.deserialize_payload_json().unwrap();
    assert_eq!(val["key"], "value");
}

#[test]
fn upcaster_registered_after_append_applies_retroactively() {
    // Existing events in the store get upcast when a new upcaster is registered.
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/memory/retro", "test", None)
        .unwrap();

    let id = store
        .append(Append {
            stream_id: "cerebra/memory/retro".to_string(),
            event_type: "MemoryScored".to_string(),
            type_version: 1,
            payload: serde_json::json!({"score": 55}),
            ..Default::default()
        })
        .unwrap();

    // Read before registration — original payload.
    let before = store.read_one(id).unwrap().unwrap();
    let val_before: serde_json::Value = before.deserialize_payload_json().unwrap();
    assert_eq!(val_before["score"], 55);

    // Register upcaster.
    store
        .register_upcaster("MemoryScored", 1, 2, ScoreV1toV2)
        .unwrap();

    // Read after registration — upcast payload.
    let after = store.read_one(id).unwrap().unwrap();
    let val_after: serde_json::Value = after.deserialize_payload_json().unwrap();
    assert!(val_after.get("score").is_none());
    assert_eq!(val_after["raw_score"], 55);
}
