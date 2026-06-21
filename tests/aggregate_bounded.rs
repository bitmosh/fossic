use fossic::{Aggregate, AggregateQuery, OpenOptions, ReadOutcome, StoredEvent, Store, TruncationReason};

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

// ── test aggregator ───────────────────────────────────────────────────────────

/// Collects event types seen; Clone is derived so aggregate_bounded can snapshot at cut points.
#[derive(Clone, Default)]
struct TypeCollector {
    types: Vec<String>,
}

impl Aggregate for TypeCollector {
    type Output = Vec<String>;
    fn fold(&mut self, event: &StoredEvent) {
        self.types.push(event.event_type.clone());
    }
    fn finalize(self) -> Vec<String> {
        self.types
    }
}

/// Sums a "value" field from each event's payload; finalize returns the sum.
#[derive(Clone, Default)]
struct Summator {
    total: i64,
}

impl Aggregate for Summator {
    type Output = i64;
    fn fold(&mut self, event: &StoredEvent) {
        if let Ok(v) = event.deserialize_payload::<serde_json::Value>() {
            if let Some(n) = v.get("value").and_then(|x| x.as_i64()) {
                self.total += n;
            }
        }
    }
    fn finalize(self) -> i64 {
        self.total
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn standard_query() -> AggregateQuery {
    AggregateQuery {
        stream_pattern: "agg/*".into(),
        branch: "main".into(),
        event_type_filter: None,
        from_timestamp_us: None,
        to_timestamp_us: None,
        indexed_tags_filter: None,
    }
}

fn populate(store: &Store, n: usize) {
    store.declare_stream("agg/s", "main", None).unwrap();
    for i in 0..n {
        store
            .append(fossic::Append {
                stream_id: "agg/s".into(),
                branch: "main".into(),
                event_type: format!("E{i}"),
                payload: serde_json::json!({ "value": i as i64, "i": i }),
                ..Default::default()
            })
            .unwrap();
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn aggregate_bounded_no_budget_returns_complete() {
    let store = temp_store();
    populate(&store, 4);
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(types) => assert_eq!(types.len(), 4),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn aggregate_bounded_empty_stream_returns_complete_empty() {
    let store = temp_store();
    store.declare_stream("agg/s", "main", None).unwrap();
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(types) => assert!(types.is_empty()),
        ReadOutcome::Truncated { .. } => panic!("expected Complete(empty)"),
    }
}

#[test]
fn aggregate_bounded_event_count_truncation() {
    let store = temp_store();
    populate(&store, 5);
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), Some(3), None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, cursor } => {
            assert_eq!(data.len(), 3, "exactly 3 events folded before cut");
            assert_eq!(reason, TruncationReason::ResultCount);
            assert!(cursor.is_none(), "aggregate_bounded cursor is always None");
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn aggregate_bounded_complete_when_exactly_at_limit() {
    let store = temp_store();
    populate(&store, 3);
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), Some(3), None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(types) => assert_eq!(types.len(), 3),
        ReadOutcome::Truncated { .. } => panic!("expected Complete — limit == event count"),
    }
}

#[test]
fn aggregate_bounded_byte_truncation_at_least_one() {
    let store = temp_store();
    populate(&store, 4);
    // 1-byte budget: first event always folds (at-least-one guarantee);
    // second event triggers the byte cut.
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), None, Some(1))
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, cursor } => {
            assert_eq!(data.len(), 1, "at-least-one: exactly one event folded");
            assert_eq!(reason, TruncationReason::ByteSize);
            assert!(cursor.is_none());
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn aggregate_bounded_count_wins_over_bytes() {
    let store = temp_store();
    populate(&store, 6);
    // Both budgets set; event_count=2 fires before bytes for small events.
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), Some(2), Some(1_000_000))
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, .. } => {
            assert_eq!(data.len(), 2);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn aggregate_bounded_finalize_accumulates_correct_state() {
    let store = temp_store();
    populate(&store, 5); // values 0..4, sum = 10
    let outcome = store
        .aggregate_bounded(standard_query(), Summator::default(), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(total) => assert_eq!(total, 10),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn aggregate_bounded_truncated_finalizes_partial_state() {
    let store = temp_store();
    populate(&store, 5); // values 0..4; first 3 = 0+1+2 = 3
    let outcome = store
        .aggregate_bounded(standard_query(), Summator::default(), Some(3), None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, reason, cursor } => {
            assert_eq!(data, 3, "partial finalize: sum of first 3 events (0+1+2)");
            assert_eq!(reason, TruncationReason::ResultCount);
            assert!(cursor.is_none());
        }
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn aggregate_bounded_uses_store_default_max_results() {
    let store = temp_store_with_defaults(Some(2), None);
    populate(&store, 5);
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), None, None)
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
fn aggregate_bounded_per_call_overrides_store_default() {
    let store = temp_store_with_defaults(Some(2), None);
    populate(&store, 5);
    // Per-call limit of 4 overrides the store default of 2.
    let outcome = store
        .aggregate_bounded(standard_query(), TypeCollector::default(), Some(4), None)
        .unwrap();
    match outcome {
        ReadOutcome::Truncated { data, .. } => assert_eq!(data.len(), 4),
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
    }
}

#[test]
fn aggregate_bounded_event_type_filter_respected() {
    let store = temp_store();
    store.declare_stream("agg/s", "main", None).unwrap();
    for i in 0..4_u32 {
        let et = if i % 2 == 0 { "Even" } else { "Odd" };
        store
            .append(fossic::Append {
                stream_id: "agg/s".into(),
                branch: "main".into(),
                event_type: et.into(),
                payload: serde_json::json!({ "i": i }),
                ..Default::default()
            })
            .unwrap();
    }
    let q = AggregateQuery {
        stream_pattern: "agg/*".into(),
        branch: "main".into(),
        event_type_filter: Some("Even".into()),
        from_timestamp_us: None,
        to_timestamp_us: None,
        indexed_tags_filter: None,
    };
    let outcome = store
        .aggregate_bounded(q, TypeCollector::default(), None, None)
        .unwrap();
    match outcome {
        ReadOutcome::Complete(types) => {
            assert_eq!(types.len(), 2);
            assert!(types.iter().all(|t| t == "Even"));
        }
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}
