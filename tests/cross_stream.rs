use fossic::{
    Aggregate, AggregateQuery, Append, EventId, OpenOptions, Store, StoredEvent, WalkDirection,
};
use std::collections::HashMap;

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

fn declare_and_append(
    store: &Store,
    stream: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> EventId {
    store.declare_stream(stream, "test", None).ok();
    store
        .append(Append {
            stream_id: stream.to_string(),
            event_type: event_type.to_string(),
            type_version: 1,
            payload,
            ..Default::default()
        })
        .unwrap()
}

// ── read_by_correlation ───────────────────────────────────────────────────────

#[test]
fn read_by_correlation_finds_events_across_streams() {
    let (store, _dir) = open_tmp();
    store.declare_stream("stream/a", "test", None).unwrap();
    store.declare_stream("stream/b", "test", None).unwrap();

    // Derive a shared correlation id from an arbitrary event id.
    let corr_src = store
        .append(Append {
            stream_id: "stream/a".to_string(),
            event_type: "Root".to_string(),
            type_version: 1,
            payload: serde_json::json!({"v": 0}),
            ..Default::default()
        })
        .unwrap();

    let corr_id = corr_src; // reuse root id as correlation id

    // Append two more events with that correlation id, on different streams.
    for (stream, tag) in &[("stream/a", "A"), ("stream/b", "B")] {
        store
            .append(Append {
                stream_id: stream.to_string(),
                event_type: "Tagged".to_string(),
                type_version: 1,
                payload: serde_json::json!({"tag": tag}),
                correlation_id: Some(corr_id),
                ..Default::default()
            })
            .unwrap();
    }

    let related = store.read_by_correlation(corr_id).unwrap();
    assert_eq!(related.len(), 2);
    // Both come back; streams can be different.
    let streams: Vec<&str> = related.iter().map(|e| e.stream_id.as_str()).collect();
    assert!(streams.contains(&"stream/a"));
    assert!(streams.contains(&"stream/b"));
}

#[test]
fn read_by_correlation_empty_when_none_match() {
    let (store, _dir) = open_tmp();
    store.declare_stream("stream/x", "test", None).unwrap();
    let id = declare_and_append(&store, "stream/x", "E", serde_json::json!({}));
    // Query with the id itself as correlation (no events have it as correlation_id).
    let results = store.read_by_correlation(id).unwrap();
    assert!(results.is_empty());
}

// ── walk_causation ────────────────────────────────────────────────────────────

/// Build a causation chain: root → child1 → child2 → child3.
fn build_chain(store: &Store) -> Vec<EventId> {
    store.declare_stream("chain/events", "test", None).unwrap();
    let mut ids = Vec::new();
    let root = store
        .append(Append {
            stream_id: "chain/events".to_string(),
            event_type: "Node".to_string(),
            type_version: 1,
            payload: serde_json::json!({"depth": 0}),
            ..Default::default()
        })
        .unwrap();
    ids.push(root);
    let mut prev = root;
    for depth in 1..=3 {
        let id = store
            .append(Append {
                stream_id: "chain/events".to_string(),
                event_type: "Node".to_string(),
                type_version: 1,
                payload: serde_json::json!({"depth": depth}),
                causation_id: Some(prev),
                ..Default::default()
            })
            .unwrap();
        ids.push(id);
        prev = id;
    }
    ids // [root, d1, d2, d3]
}

#[test]
fn walk_causation_forward_depth3() {
    let (store, _dir) = open_tmp();
    let ids = build_chain(&store);
    let root = ids[0];

    let results = store
        .walk_causation(root, WalkDirection::Forward, 3)
        .unwrap();
    // Should find d1, d2, d3 (3 descendants).
    assert_eq!(
        results.len(),
        3,
        "forward depth=3 must return 3 descendants"
    );

    // BFS order: shallowest first.
    let depths: Vec<i64> = results
        .iter()
        .map(|e| {
            let v: serde_json::Value = e.deserialize_payload_json().unwrap();
            v["depth"].as_i64().unwrap()
        })
        .collect();
    assert_eq!(depths, vec![1, 2, 3]);
}

#[test]
fn walk_causation_forward_depth_limited() {
    let (store, _dir) = open_tmp();
    let ids = build_chain(&store);

    let results = store
        .walk_causation(ids[0], WalkDirection::Forward, 2)
        .unwrap();
    assert_eq!(
        results.len(),
        2,
        "depth limit=2 must return only 2 descendants"
    );
    let depths: Vec<i64> = results
        .iter()
        .map(|e| {
            let v: serde_json::Value = e.deserialize_payload_json().unwrap();
            v["depth"].as_i64().unwrap()
        })
        .collect();
    assert_eq!(depths, vec![1, 2]);
}

#[test]
fn walk_causation_backward_to_root() {
    let (store, _dir) = open_tmp();
    let ids = build_chain(&store);
    let leaf = ids[3]; // d3

    let results = store
        .walk_causation(leaf, WalkDirection::Backward, 10)
        .unwrap();
    // Should find d2, d1, root (3 ancestors).
    assert_eq!(
        results.len(),
        3,
        "backward from leaf must find all ancestors"
    );

    let depths: Vec<i64> = results
        .iter()
        .map(|e| {
            let v: serde_json::Value = e.deserialize_payload_json().unwrap();
            v["depth"].as_i64().unwrap()
        })
        .collect();
    assert_eq!(depths, vec![2, 1, 0], "ancestors in BFS order from leaf");
}

#[test]
fn walk_causation_both_direction() {
    let (store, _dir) = open_tmp();
    let ids = build_chain(&store);
    let middle = ids[1]; // d1: parent=root, child=d2→d3

    let results = store
        .walk_causation(middle, WalkDirection::Both, 10)
        .unwrap();
    // Forward from d1: d2, d3; Backward from d1: root
    let result_ids: Vec<[u8; 32]> = results.iter().map(|e| *e.id.as_bytes()).collect();

    assert!(result_ids.contains(ids[0].as_bytes()), "root must appear");
    assert!(result_ids.contains(ids[2].as_bytes()), "d2 must appear");
    assert!(result_ids.contains(ids[3].as_bytes()), "d3 must appear");
    assert_eq!(results.len(), 3);
}

#[test]
fn walk_causation_forward_max_depth_zero_returns_empty() {
    let (store, _dir) = open_tmp();
    let ids = build_chain(&store);
    let results = store
        .walk_causation(ids[0], WalkDirection::Forward, 0)
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn walk_causation_no_children_returns_empty() {
    let (store, _dir) = open_tmp();
    store.declare_stream("lone/events", "test", None).unwrap();
    let id = store
        .append(Append {
            stream_id: "lone/events".to_string(),
            event_type: "Lone".to_string(),
            type_version: 1,
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let results = store.walk_causation(id, WalkDirection::Forward, 5).unwrap();
    assert!(results.is_empty());
}

// ── aggregate ─────────────────────────────────────────────────────────────────

/// Counts events grouped by a string "group" field in indexed_tags.
struct CountByGroup(HashMap<String, u64>);

impl Aggregate for CountByGroup {
    type Output = HashMap<String, u64>;

    fn fold(&mut self, event: &StoredEvent) {
        if let Some(tags) = &event.indexed_tags {
            if let Some(g) = tags.get("group").and_then(|v| v.as_str()) {
                *self.0.entry(g.to_string()).or_insert(0) += 1;
            }
        }
    }

    fn finalize(self) -> Self::Output {
        self.0
    }
}

#[test]
fn aggregate_counts_events_by_group() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();

    let mut seq = 0u64;
    for (group, n) in &[("arm_A", 3u64), ("arm_B", 2u64)] {
        for _ in 0..*n {
            // Each event needs a unique payload so CCE produces distinct IDs.
            seq += 1;
            store
                .append(Append {
                    stream_id: "bonsai/ideas".to_string(),
                    event_type: "IdeaScored".to_string(),
                    type_version: 1,
                    payload: serde_json::json!({"seq": seq}),
                    indexed_tags: Some(serde_json::json!({"group": group})),
                    ..Default::default()
                })
                .unwrap();
        }
    }

    let counts = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/ideas".to_string(),
                ..Default::default()
            },
            CountByGroup(HashMap::new()),
        )
        .unwrap();

    assert_eq!(counts["arm_A"], 3);
    assert_eq!(counts["arm_B"], 2);
}

#[test]
fn aggregate_empty_result_when_no_events_match() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();

    let counts = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/other_pattern".to_string(),
                ..Default::default()
            },
            CountByGroup(HashMap::new()),
        )
        .unwrap();
    assert!(counts.is_empty());
}

#[test]
fn aggregate_event_type_filter() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();

    store
        .append(Append {
            stream_id: "bonsai/ideas".to_string(),
            event_type: "IdeaCreated".to_string(),
            type_version: 1,
            payload: serde_json::json!({"seq": 1}),
            indexed_tags: Some(serde_json::json!({"group": "x"})),
            ..Default::default()
        })
        .unwrap();
    store
        .append(Append {
            stream_id: "bonsai/ideas".to_string(),
            event_type: "IdeaScored".to_string(),
            type_version: 1,
            payload: serde_json::json!({"seq": 2}),
            indexed_tags: Some(serde_json::json!({"group": "y"})),
            ..Default::default()
        })
        .unwrap();

    let counts = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/ideas".to_string(),
                event_type_filter: Some("IdeaScored".to_string()),
                ..Default::default()
            },
            CountByGroup(HashMap::new()),
        )
        .unwrap();

    assert_eq!(counts.len(), 1);
    assert_eq!(counts["y"], 1);
}

// ── indexed_tags_filter: SQL-level filtering ──────────────────────────────────

#[test]
fn aggregate_indexed_tags_string_filter() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();

    for (group, n) in &[("arm_A", 3u64), ("arm_B", 2u64)] {
        for i in 0..*n {
            store
                .append(Append {
                    stream_id: "bonsai/ideas".to_string(),
                    event_type: "IdeaScored".to_string(),
                    type_version: 1,
                    payload: serde_json::json!({ "i": i }),
                    indexed_tags: Some(serde_json::json!({ "group": group })),
                    ..Default::default()
                })
                .unwrap();
        }
    }

    let results = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/ideas".to_string(),
                indexed_tags_filter: Some(serde_json::json!({ "group": "arm_A" })),
                ..Default::default()
            },
            CountByGroup(HashMap::new()),
        )
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results["arm_A"], 3);
}

#[test]
fn aggregate_indexed_tags_boolean_filter() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();

    struct CountEvents(u64);
    impl Aggregate for CountEvents {
        type Output = u64;
        fn fold(&mut self, _: &StoredEvent) {
            self.0 += 1;
        }
        fn finalize(self) -> u64 {
            self.0
        }
    }

    // Use a global seq in payload to prevent CCE dedup across the two groups.
    // indexed_tags is NOT part of the CCE hash, so same payload + different tags
    // produces the same event ID and the second append is a no-op.
    let mut seq = 0u64;
    for (violated, n) in &[(true, 2u64), (false, 3u64)] {
        for _ in 0..*n {
            seq += 1;
            store
                .append(Append {
                    stream_id: "bonsai/ideas".to_string(),
                    event_type: "IdeaScored".to_string(),
                    type_version: 1,
                    payload: serde_json::json!({ "seq": seq }),
                    indexed_tags: Some(
                        serde_json::json!({ "composite_floor_violated": *violated }),
                    ),
                    ..Default::default()
                })
                .unwrap();
        }
    }

    let count_true = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/ideas".to_string(),
                indexed_tags_filter: Some(serde_json::json!({ "composite_floor_violated": true })),
                ..Default::default()
            },
            CountEvents(0),
        )
        .unwrap();
    assert_eq!(count_true, 2);

    let count_false = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/ideas".to_string(),
                indexed_tags_filter: Some(serde_json::json!({ "composite_floor_violated": false })),
                ..Default::default()
            },
            CountEvents(0),
        )
        .unwrap();
    assert_eq!(count_false, 3);
}

#[test]
fn aggregate_indexed_tags_multi_key_and() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();

    // Three events: only the one with BOTH session_id=s1 AND arm_id=a1 should match.
    for (session, arm, i) in [("s1", "a1", 0), ("s1", "a2", 1), ("s2", "a1", 2)] {
        store
            .append(Append {
                stream_id: "bonsai/ideas".to_string(),
                event_type: "Ev".to_string(),
                type_version: 1,
                payload: serde_json::json!({ "i": i }),
                indexed_tags: Some(serde_json::json!({ "session_id": session, "arm_id": arm })),
                ..Default::default()
            })
            .unwrap();
    }

    struct CountEvents(u64);
    impl Aggregate for CountEvents {
        type Output = u64;
        fn fold(&mut self, _: &StoredEvent) {
            self.0 += 1;
        }
        fn finalize(self) -> u64 {
            self.0
        }
    }

    let count = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/ideas".to_string(),
                indexed_tags_filter: Some(
                    serde_json::json!({ "session_id": "s1", "arm_id": "a1" }),
                ),
                ..Default::default()
            },
            CountEvents(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn aggregate_indexed_tags_filter_no_match_returns_empty() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();

    store
        .append(Append {
            stream_id: "bonsai/ideas".to_string(),
            event_type: "Ev".to_string(),
            type_version: 1,
            payload: serde_json::json!({ "i": 1 }),
            indexed_tags: Some(serde_json::json!({ "group": "arm_A" })),
            ..Default::default()
        })
        .unwrap();

    let counts = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/ideas".to_string(),
                indexed_tags_filter: Some(serde_json::json!({ "group": "nonexistent" })),
                ..Default::default()
            },
            CountByGroup(HashMap::new()),
        )
        .unwrap();
    assert!(counts.is_empty());
}

// ── Glob semantics fix: * must not cross segment boundaries ──────────────────

#[test]
fn aggregate_single_star_does_not_cross_segments() {
    let (store, _dir) = open_tmp();
    store.declare_stream("bonsai/ideas", "test", None).unwrap();
    store
        .declare_stream("bonsai/ideas/nested", "test", None)
        .unwrap();

    store
        .append(Append {
            stream_id: "bonsai/ideas".to_string(),
            event_type: "Ev".to_string(),
            payload: serde_json::json!({ "depth": 1 }),
            ..Default::default()
        })
        .unwrap();
    store
        .append(Append {
            stream_id: "bonsai/ideas/nested".to_string(),
            event_type: "Ev".to_string(),
            payload: serde_json::json!({ "depth": 2 }),
            ..Default::default()
        })
        .unwrap();

    struct CountEvents(u64);
    impl Aggregate for CountEvents {
        type Output = u64;
        fn fold(&mut self, _: &StoredEvent) {
            self.0 += 1;
        }
        fn finalize(self) -> u64 {
            self.0
        }
    }

    // "bonsai/*" should match "bonsai/ideas" but NOT "bonsai/ideas/nested"
    let count = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/*".to_string(),
                ..Default::default()
            },
            CountEvents(0),
        )
        .unwrap();
    assert_eq!(count, 1, "bonsai/* must not match bonsai/ideas/nested");

    // "bonsai/**" should match both
    let count_deep = store
        .aggregate(
            AggregateQuery {
                stream_pattern: "bonsai/**".to_string(),
                ..Default::default()
            },
            CountEvents(0),
        )
        .unwrap();
    assert_eq!(count_deep, 2, "bonsai/** must match both streams");
}

// ── D2/D3 regression: usize::MAX must not corrupt depth bound ─────────────────

#[test]
fn walk_causation_forward_unbounded_depth() {
    // Regression for usize::MAX as i64 = -1 on 64-bit platforms, which caused
    // the CTE WHERE clause to be always-false and return only depth-1 results.
    let (store, _dir) = open_tmp();
    let ids = build_chain(&store); // [root, d1, d2, d3]

    // usize::MAX is the "no limit" sentinel — all 3 descendants must be returned.
    let results = store
        .walk_causation(ids[0], WalkDirection::Forward, usize::MAX)
        .unwrap();
    assert_eq!(
        results.len(),
        3,
        "usize::MAX depth must return all descendants, got {}",
        results.len()
    );
}
