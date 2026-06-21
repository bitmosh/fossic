use fossic::{
    Append, BudgetKind, OpenOptions, ReadOutcome, SamplingMode, Store, TruncationCursor,
    TruncationReason,
};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("test.db"), OpenOptions::default())
        .expect("open store");
    (store, dir)
}

// ── OpenOptions default field values ────────────────────────────────────────

#[test]
fn open_options_default_max_fields_are_none() {
    let opts = OpenOptions::default();
    assert!(opts.default_max_results.is_none());
    assert!(opts.default_max_bytes.is_none());
}

#[test]
fn open_options_max_fields_round_trip() {
    let opts = OpenOptions {
        default_max_results: Some(1000),
        default_max_bytes: Some(512 * 1024),
        ..Default::default()
    };
    assert_eq!(opts.default_max_results, Some(1000));
    assert_eq!(opts.default_max_bytes, Some(512 * 1024));
}

// ── BudgetKind discriminant ───────────────────────────────────────────────────

#[test]
fn budget_kind_variants_are_distinct() {
    assert_ne!(BudgetKind::ResultCount, BudgetKind::ByteSize);
}

#[test]
fn budget_kind_copy() {
    let a = BudgetKind::ResultCount;
    let b = a; // Copy
    assert_eq!(a, b);
}

// ── TruncationReason ──────────────────────────────────────────────────────────

#[test]
fn truncation_reason_variants_are_distinct() {
    assert_ne!(TruncationReason::ResultCount, TruncationReason::ByteSize);
}

// ── ReadOutcome ───────────────────────────────────────────────────────────────

#[test]
fn read_outcome_complete_wraps_value() {
    let outcome: ReadOutcome<Vec<u32>> = ReadOutcome::Complete(vec![1, 2, 3]);
    match outcome {
        ReadOutcome::Complete(v) => assert_eq!(v, vec![1, 2, 3]),
        ReadOutcome::Truncated { .. } => panic!("expected Complete"),
    }
}

#[test]
fn read_outcome_truncated_carries_cursor_and_reason() {
    let cursor = TruncationCursor::from_bytes(vec![0xDE, 0xAD]);
    let outcome: ReadOutcome<Vec<u32>> = ReadOutcome::Truncated {
        data: vec![1],
        cursor,
        reason: TruncationReason::ResultCount,
    };
    match outcome {
        ReadOutcome::Complete(_) => panic!("expected Truncated"),
        ReadOutcome::Truncated { data, cursor, reason } => {
            assert_eq!(data, vec![1]);
            assert_eq!(cursor.as_bytes(), &[0xDE, 0xAD]);
            assert_eq!(reason, TruncationReason::ResultCount);
        }
    }
}

// ── TruncationCursor round-trip ───────────────────────────────────────────────

#[test]
fn truncation_cursor_bytes_round_trip() {
    let raw = vec![1u8, 2, 3, 4, 5];
    let cursor = TruncationCursor::from_bytes(raw.clone());
    assert_eq!(cursor.as_bytes(), raw.as_slice());
    assert_eq!(cursor.into_bytes(), raw);
}

#[test]
fn truncation_cursor_empty_bytes() {
    let cursor = TruncationCursor::from_bytes(vec![]);
    assert!(cursor.as_bytes().is_empty());
    assert!(cursor.into_bytes().is_empty());
}

// ── SamplingMode ──────────────────────────────────────────────────────────────

#[test]
fn sampling_mode_exhaustive() {
    let m = SamplingMode::Exhaustive;
    assert_eq!(m, SamplingMode::Exhaustive);
}

#[test]
fn sampling_mode_breadth_first_carries_limit() {
    let m = SamplingMode::BreadthFirst { max_per_level: 10 };
    match m {
        SamplingMode::BreadthFirst { max_per_level } => assert_eq!(max_per_level, 10),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn sampling_mode_adaptive_carries_target() {
    let m = SamplingMode::Adaptive { target_count: 250 };
    match m {
        SamplingMode::Adaptive { target_count } => assert_eq!(target_count, 250),
        _ => panic!("wrong variant"),
    }
}

// ── dispatch_channel observability ───────────────────────────────────────────

#[test]
fn dispatch_channel_pressure_starts_at_zero() {
    let (store, _dir) = open_tmp();
    assert_eq!(store.dispatch_channel_pressure(), 0);
}

#[test]
fn dispatch_channel_hwm_starts_at_zero() {
    let (store, _dir) = open_tmp();
    assert_eq!(store.dispatch_channel_high_water_mark(), 0);
}

#[test]
fn dispatch_channel_hwm_updates_after_append_with_subscriber() {
    use fossic::{SubscribeQuery, SubscriptionMode};
    use std::sync::{Arc, Mutex};

    let (store, _dir) = open_tmp();
    store.declare_stream("obs/stream", "test", None).unwrap();

    let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recv_clone = Arc::clone(&received);

    struct Collector(Arc<Mutex<Vec<String>>>);
    impl fossic::SubscriptionHandler for Collector {
        fn on_event(&self, event: &fossic::StoredEvent) {
            self.0.lock().unwrap().push(event.event_type.clone());
        }
    }

    // PostCommit with a generous queue so it doesn't degrade.
    let _handle = store
        .subscribe(
            SubscribeQuery::stream("obs/stream"),
            SubscriptionMode::PostCommit { queue_size: 64 },
            Collector(recv_clone),
        )
        .unwrap();

    store
        .append(Append {
            stream_id: "obs/stream".to_string(),
            event_type: "Ping".to_string(),
            type_version: 1,
            payload: serde_json::json!({"n": 1}),
            ..Default::default()
        })
        .unwrap();

    // HWM should be at least 1 — we sent one event through the channel.
    assert!(
        store.dispatch_channel_high_water_mark() >= 1,
        "hwm must be ≥ 1 after a dispatched event, got {}",
        store.dispatch_channel_high_water_mark()
    );
}
