use fossic::{
    Append, EventId, OpenOptions, ReadOutcome, ReadQuery, SamplingMode, Store, TruncationReason,
    WalkDirection,
};

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

/// Linear chain: root → d1 → d2 → d3. Returns [root, d1, d2, d3].
fn build_chain(store: &Store) -> Vec<EventId> {
    store.declare_stream("chain", "main", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "chain".into(),
            branch: "main".into(),
            event_type: "Node".into(),
            payload: serde_json::json!({"depth": 0}),
            ..Default::default()
        })
        .unwrap();
    let mut ids = vec![root];
    let mut prev = root;
    for depth in 1..=3 {
        let id = store
            .append(Append {
                stream_id: "chain".into(),
                branch: "main".into(),
                event_type: "Node".into(),
                payload: serde_json::json!({"depth": depth}),
                causation_id: Some(prev),
                ..Default::default()
            })
            .unwrap();
        ids.push(id);
        prev = id;
    }
    ids
}

/// Wide tree: root → [A, B, C] (three children, no grandchildren). Returns (root, [A, B, C]).
fn build_wide_tree(store: &Store) -> (EventId, Vec<EventId>) {
    store.declare_stream("wtree", "main", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "wtree".into(),
            branch: "main".into(),
            event_type: "Node".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let children: Vec<EventId> = (0..3_u32)
        .map(|i| {
            store
                .append(Append {
                    stream_id: "wtree".into(),
                    branch: "main".into(),
                    event_type: "Node".into(),
                    payload: serde_json::json!({ "i": i }),
                    causation_id: Some(root),
                    ..Default::default()
                })
                .unwrap()
        })
        .collect();
    (root, children)
}

// ── forward / backward / both ─────────────────────────────────────────────────

#[test]
fn causation_bounded_forward_no_budget_returns_complete() {
    let store = temp_store();
    let ids = build_chain(&store);
    let outcome = store
        .walk_causation_bounded(
            ids[0],
            WalkDirection::Forward,
            10,
            SamplingMode::Exhaustive,
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 3),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn causation_bounded_backward_walk_finds_ancestors() {
    let store = temp_store();
    let ids = build_chain(&store);
    let leaf = ids[3]; // d3
    let outcome = store
        .walk_causation_bounded(
            leaf,
            WalkDirection::Backward,
            10,
            SamplingMode::Exhaustive,
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => {
            assert_eq!(events.len(), 3);
            let result_ids: Vec<[u8; 32]> = events.iter().map(|e| *e.id.as_bytes()).collect();
            assert!(result_ids.contains(ids[0].as_bytes()), "root must appear");
            assert!(result_ids.contains(ids[1].as_bytes()), "d1 must appear");
            assert!(result_ids.contains(ids[2].as_bytes()), "d2 must appear");
        }
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn causation_bounded_both_direction_walk_finds_neighbors() {
    let store = temp_store();
    let ids = build_chain(&store);
    let middle = ids[1]; // d1: parent=root, child=d2
    let outcome = store
        .walk_causation_bounded(
            middle,
            WalkDirection::Both,
            10,
            SamplingMode::Exhaustive,
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => {
            assert_eq!(events.len(), 3); // root, d2, d3
            let result_ids: Vec<[u8; 32]> = events.iter().map(|e| *e.id.as_bytes()).collect();
            assert!(result_ids.contains(ids[0].as_bytes()), "root must appear");
            assert!(result_ids.contains(ids[2].as_bytes()), "d2 must appear");
            assert!(result_ids.contains(ids[3].as_bytes()), "d3 must appear");
        }
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

// ── budget truncation ─────────────────────────────────────────────────────────

#[test]
fn causation_bounded_truncates_at_result_count() {
    let store = temp_store();
    let ids = build_chain(&store);
    // Budget=1: level 1 (d1) is yielded; level 2 would push total to 2 > 1 → Truncated.
    let outcome = store
        .walk_causation_bounded(
            ids[0],
            WalkDirection::Forward,
            10,
            SamplingMode::Exhaustive,
            Some(1),
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 1);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn causation_bounded_truncates_at_byte_budget_at_least_one() {
    let store = temp_store();
    let ids = build_chain(&store);
    // 1-byte budget: level 1 (d1) exceeds it, but at-least-one guarantee yields it
    // anyway (results.is_empty() guard). Level 2 then triggers the actual cut.
    let outcome = store
        .walk_causation_bounded(
            ids[0],
            WalkDirection::Forward,
            10,
            SamplingMode::Exhaustive,
            None,
            Some(1),
            None,
        )
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
fn causation_bounded_complete_when_exactly_at_limit() {
    let store = temp_store();
    let ids = build_chain(&store);
    // Budget = 3 == number of descendants; no level exceeds it → Complete.
    let outcome = store
        .walk_causation_bounded(
            ids[0],
            WalkDirection::Forward,
            10,
            SamplingMode::Exhaustive,
            Some(3),
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 3),
        ReadOutcome::Truncated { .. } => panic!("expected Complete — limit == event count"),
    }
}

// ── pagination via cursor resume ──────────────────────────────────────────────

#[test]
fn causation_bounded_resume_full_pagination() {
    let store = temp_store();
    let ids = build_chain(&store);
    // Budget=1 per page: page 1=d1, page 2=d2, page 3=d3.
    let mut all_ids: Vec<[u8; 32]> = Vec::new();
    let mut cursor_opt = None;
    loop {
        match store
            .walk_causation_bounded(
                ids[0],
                WalkDirection::Forward,
                10,
                SamplingMode::Exhaustive,
                Some(1),
                None,
                cursor_opt,
            )
            .unwrap()
        {
            ReadOutcome::Complete(events) => {
                all_ids.extend(events.iter().map(|e| *e.id.as_bytes()));
                break;
            }
            ReadOutcome::Truncated { data, cursor, .. } => {
                all_ids.extend(data.iter().map(|e| *e.id.as_bytes()));
                cursor_opt = Some(cursor);
            }
        }
    }
    assert_eq!(all_ids.len(), 3, "all three descendants must be collected");
    // No duplicate IDs across pages.
    let unique: std::collections::HashSet<_> = all_ids.iter().collect();
    assert_eq!(unique.len(), all_ids.len(), "no duplicates across pages");
}

// ── max_depth ─────────────────────────────────────────────────────────────────

#[test]
fn causation_bounded_max_depth_respected() {
    let store = temp_store();
    let ids = build_chain(&store);
    // max_depth=2: only d1 (level 1) and d2 (level 2) are reachable.
    let outcome = store
        .walk_causation_bounded(
            ids[0],
            WalkDirection::Forward,
            2,
            SamplingMode::Exhaustive,
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 2),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

// ── empty BFS ─────────────────────────────────────────────────────────────────

#[test]
fn causation_bounded_no_children_returns_complete_empty() {
    let store = temp_store();
    store.declare_stream("lone", "main", None).unwrap();
    let id = store
        .append(Append {
            stream_id: "lone".into(),
            branch: "main".into(),
            event_type: "Lone".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let outcome = store
        .walk_causation_bounded(
            id,
            WalkDirection::Forward,
            10,
            SamplingMode::Exhaustive,
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert!(events.is_empty()),
        ReadOutcome::Truncated { .. } => panic!("expected Complete(empty)"),
    }
}

// ── sampling modes ────────────────────────────────────────────────────────────

#[test]
fn causation_bounded_breadth_first_sampling_caps_per_level() {
    let store = temp_store();
    let (root, _children) = build_wide_tree(&store); // root → [A, B, C]
    // BreadthFirst{max_per_level:1}: level 1 has 3 events but only 1 is taken (id ASC).
    let outcome = store
        .walk_causation_bounded(
            root,
            WalkDirection::Forward,
            10,
            SamplingMode::BreadthFirst { max_per_level: 1 },
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 1),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn causation_bounded_adaptive_sampling_distributes_count() {
    let store = temp_store();
    let (root, _children) = build_wide_tree(&store); // root → [A, B, C]
    // Adaptive{target_count:2}, max_depth=1 → max_per_level = max(1, 2/1) = 2.
    // Level 1 has 3 events → capped to 2. max_depth=1 stops here → Complete([A, B]).
    let outcome = store
        .walk_causation_bounded(
            root,
            WalkDirection::Forward,
            1,
            SamplingMode::Adaptive { target_count: 2 },
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Complete(events) => assert_eq!(events.len(), 2),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

// ── store defaults ────────────────────────────────────────────────────────────

#[test]
fn causation_bounded_uses_store_default_max_results() {
    let store = temp_store_with_defaults(Some(1), None);
    let ids = build_chain(&store);
    let outcome = store
        .walk_causation_bounded(
            ids[0],
            WalkDirection::Forward,
            10,
            SamplingMode::Exhaustive,
            None,
            None,
            None,
        )
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 1);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated via store default"),
    }
}

// ── error cases ───────────────────────────────────────────────────────────────

#[test]
fn causation_bounded_wrong_cursor_type_returns_error() {
    let store = temp_store();
    let ids = build_chain(&store);
    // Build a Range cursor from the chain stream (4 events; limit=1 → Truncated).
    let range_cursor = match store
        .read_range_bounded(ReadQuery::stream("chain"), Some(1), None, None)
        .unwrap()
    {
        ReadOutcome::Truncated { cursor, .. } => cursor,
        ReadOutcome::Complete(_) => panic!("chain has 4 events; limit=1 must truncate"),
    };
    let result = store.walk_causation_bounded(
        ids[0],
        WalkDirection::Forward,
        10,
        SamplingMode::Exhaustive,
        None,
        None,
        Some(range_cursor),
    );
    assert!(result.is_err(), "mismatched cursor type must return Err");
}

#[test]
fn causation_bounded_cursor_direction_mismatch_returns_error() {
    let store = temp_store();
    let ids = build_chain(&store);
    // Obtain a Forward cursor (budget=1 → Truncated after d1).
    let fwd_cursor = match store
        .walk_causation_bounded(
            ids[0],
            WalkDirection::Forward,
            10,
            SamplingMode::Exhaustive,
            Some(1),
            None,
            None,
        )
        .unwrap()
    {
        ReadOutcome::Truncated { cursor, .. } => cursor,
        ReadOutcome::Complete(_) => panic!("expected Truncated to get a cursor"),
    };
    // Pass Forward cursor to a Backward call — direction mismatch must error.
    let result = store.walk_causation_bounded(
        ids[3],
        WalkDirection::Backward,
        10,
        SamplingMode::Exhaustive,
        None,
        None,
        Some(fwd_cursor),
    );
    assert!(result.is_err(), "direction mismatch must return Err");
}
