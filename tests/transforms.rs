use fossic::{Append, Error, OpenOptions, PayloadTransform, Store};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

// ── Transform helpers ─────────────────────────────────────────────────────────

/// Removes the "secret" key from any msgpack object payload.
struct RedactSecret;

impl PayloadTransform for RedactSecret {
    fn transform(&self, _event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut val: serde_json::Value = rmp_serde::from_slice(payload).unwrap();
        if let Some(obj) = val.as_object_mut() {
            obj.remove("secret");
        }
        Ok(rmp_serde::to_vec(&val).unwrap())
    }
}

/// Appends "_step1" to a string field "tag".
struct AppendStep1;

impl PayloadTransform for AppendStep1 {
    fn transform(&self, _event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut val: serde_json::Value = rmp_serde::from_slice(payload).unwrap();
        if let Some(obj) = val.as_object_mut() {
            if let Some(t) = obj.get("tag").and_then(|v| v.as_str()) {
                let new_t = format!("{t}_step1");
                obj.insert("tag".to_string(), serde_json::Value::String(new_t));
            }
        }
        Ok(rmp_serde::to_vec(&val).unwrap())
    }
}

/// Appends "_step2" to a string field "tag".
struct AppendStep2;

impl PayloadTransform for AppendStep2 {
    fn transform(&self, _event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut val: serde_json::Value = rmp_serde::from_slice(payload).unwrap();
        if let Some(obj) = val.as_object_mut() {
            if let Some(t) = obj.get("tag").and_then(|v| v.as_str()) {
                let new_t = format!("{t}_step2");
                obj.insert("tag".to_string(), serde_json::Value::String(new_t));
            }
        }
        Ok(rmp_serde::to_vec(&val).unwrap())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn transform_changes_event_id() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("policy-scout/audit", "test", None)
        .unwrap();

    // Append WITHOUT transform.
    let payload = serde_json::json!({"data": "hello", "secret": "top_secret"});
    let id_plain = store
        .append(Append {
            stream_id: "policy-scout/audit".to_string(),
            event_type: "AuditEvent".to_string(),
            type_version: 1,
            payload: payload.clone(),
            ..Default::default()
        })
        .unwrap();

    // Register transform and append same logical payload to a second store.
    let dir2 = tempfile::tempdir().unwrap();
    let store2 = Store::open(dir2.path().join("t.db"), OpenOptions::default()).unwrap();
    store2
        .declare_stream("policy-scout/audit", "test", None)
        .unwrap();
    store2
        .register_payload_transform("policy-scout/audit", RedactSecret)
        .unwrap();

    let id_transformed = store2
        .append(Append {
            stream_id: "policy-scout/audit".to_string(),
            event_type: "AuditEvent".to_string(),
            type_version: 1,
            payload: payload.clone(),
            ..Default::default()
        })
        .unwrap();

    // IDs must differ because the bytes that get hashed are different.
    assert_ne!(id_plain, id_transformed, "transform must change event id");
}

#[test]
fn transformed_payload_stored_without_secret() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("policy-scout/audit", "test", None)
        .unwrap();
    store
        .register_payload_transform("policy-scout/audit", RedactSecret)
        .unwrap();

    let id = store
        .append(Append {
            stream_id: "policy-scout/audit".to_string(),
            event_type: "AuditEvent".to_string(),
            type_version: 1,
            payload: serde_json::json!({"data": "hello", "secret": "top_secret"}),
            ..Default::default()
        })
        .unwrap();

    let event = store.read_one(id).unwrap().unwrap();
    let stored: serde_json::Value = event.deserialize_payload_json().unwrap();
    assert!(stored.get("secret").is_none(), "secret must be redacted");
    assert_eq!(stored["data"], "hello");
}

#[test]
fn multiple_transforms_chain_in_registration_order() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/chain", "test", None).unwrap();
    store
        .register_payload_transform("test/chain", AppendStep1)
        .unwrap();
    store
        .register_payload_transform("test/chain", AppendStep2)
        .unwrap();

    let id = store
        .append(Append {
            stream_id: "test/chain".to_string(),
            event_type: "TagEvent".to_string(),
            type_version: 1,
            payload: serde_json::json!({"tag": "original"}),
            ..Default::default()
        })
        .unwrap();

    let event = store.read_one(id).unwrap().unwrap();
    let stored: serde_json::Value = event.deserialize_payload_json().unwrap();
    // Both transforms applied in order: "original_step1_step2"
    assert_eq!(stored["tag"], "original_step1_step2");
}

#[test]
fn transform_does_not_fire_for_non_matching_stream() {
    let (store, _dir) = open_tmp();
    store.declare_stream("other/stream", "test", None).unwrap();
    store
        .register_payload_transform("policy-scout/audit", RedactSecret)
        .unwrap();

    let id = store
        .append(Append {
            stream_id: "other/stream".to_string(),
            event_type: "AuditEvent".to_string(),
            type_version: 1,
            payload: serde_json::json!({"data": "hello", "secret": "still_here"}),
            ..Default::default()
        })
        .unwrap();

    let event = store.read_one(id).unwrap().unwrap();
    let stored: serde_json::Value = event.deserialize_payload_json().unwrap();
    assert_eq!(
        stored["secret"], "still_here",
        "non-matching stream must not be transformed"
    );
}

#[test]
fn wildcard_pattern_matches_all_segments() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/lattice/abc", "test", None)
        .unwrap();
    store
        .declare_stream("cerebra/lattice/def", "test", None)
        .unwrap();
    store
        .register_payload_transform("cerebra/lattice/*", RedactSecret)
        .unwrap();

    for stream in &["cerebra/lattice/abc", "cerebra/lattice/def"] {
        let id = store
            .append(Append {
                stream_id: stream.to_string(),
                event_type: "RecordEvent".to_string(),
                type_version: 1,
                payload: serde_json::json!({"data": "x", "secret": "hidden"}),
                ..Default::default()
            })
            .unwrap();

        let event = store.read_one(id).unwrap().unwrap();
        let stored: serde_json::Value = event.deserialize_payload_json().unwrap();
        assert!(
            stored.get("secret").is_none(),
            "secret must be redacted from {stream}"
        );
    }
}
