use fossic::{Append, EventId, OpenOptions, ReadQuery, SamplingMode, Store, WalkDirection};

fn temp_store() -> Store {
    let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    Store::open(path, OpenOptions::default()).unwrap()
}

fn temp_store_pool(size: usize) -> Store {
    let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    Store::open(
        path,
        OpenOptions {
            read_pool_size: size,
            read_pool_timeout_ms: 500,
            ..Default::default()
        },
    )
    .unwrap()
}

fn append_n(store: &Store, n: usize) {
    store.declare_stream("s", "main", None).unwrap();
    for i in 0..n {
        store
            .append(Append {
                stream_id: "s".into(),
                branch: "main".into(),
                event_type: "E".into(),
                payload: serde_json::json!({ "i": i }),
                ..Default::default()
            })
            .unwrap();
    }
}

fn append_correlated(store: &Store, n: usize) -> EventId {
    store.declare_stream("c", "main", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "c".into(),
            branch: "main".into(),
            event_type: "Root".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    for i in 0..n {
        store
            .append(Append {
                stream_id: "c".into(),
                branch: "main".into(),
                event_type: "Child".into(),
                payload: serde_json::json!({ "i": i }),
                correlation_id: Some(root),
                ..Default::default()
            })
            .unwrap();
    }
    root
}

fn build_chain(store: &Store) -> Vec<EventId> {
    store.declare_stream("chain", "main", None).unwrap();
    let root = store
        .append(Append {
            stream_id: "chain".into(),
            branch: "main".into(),
            event_type: "Node".into(),
            payload: serde_json::json!({ "depth": 0 }),
            ..Default::default()
        })
        .unwrap();
    let mut ids = vec![root];
    let mut prev = root;
    for depth in 1..=4_u32 {
        let id = store
            .append(Append {
                stream_id: "chain".into(),
                branch: "main".into(),
                event_type: "Node".into(),
                payload: serde_json::json!({ "depth": depth }),
                causation_id: Some(prev),
                ..Default::default()
            })
            .unwrap();
        ids.push(id);
        prev = id;
    }
    ids
}

// ── RangeIter ─────────────────────────────────────────────────────────────────

#[test]
fn range_iter_empty_stream_returns_no_items() {
    let store = temp_store();
    store.declare_stream("s", "main", None).unwrap();
    let items: Vec<_> = store.read_range_iter(ReadQuery::stream("s")).collect();
    assert!(items.is_empty());
}

#[test]
fn range_iter_collects_all_events_in_version_order() {
    let store = temp_store();
    append_n(&store, 5);
    let versions: Vec<u64> = store
        .read_range_iter(ReadQuery::stream("s"))
        .map(|r| r.unwrap().version)
        .collect();
    assert_eq!(versions, vec![0, 1, 2, 3, 4]);
}

#[test]
fn range_iter_respects_from_version() {
    let store = temp_store();
    append_n(&store, 5);
    let mut q = ReadQuery::stream("s");
    q.from_version = Some(2);
    let versions: Vec<u64> = store
        .read_range_iter(q)
        .map(|r| r.unwrap().version)
        .collect();
    assert_eq!(versions, vec![2, 3, 4]);
}

#[test]
fn range_iter_fused_after_exhaustion() {
    let store = temp_store();
    append_n(&store, 2);
    let mut iter = store.read_range_iter(ReadQuery::stream("s"));
    assert!(iter.next().is_some());
    assert!(iter.next().is_some());
    assert!(iter.next().is_none());
    assert!(
        iter.next().is_none(),
        "fused: must return None after exhaustion"
    );
    assert!(
        iter.next().is_none(),
        "fused: must return None on repeated calls"
    );
}

#[test]
fn range_iter_across_batch_boundary() {
    let store = temp_store();
    // 105 events crosses the internal ITER_BATCH_SIZE=100 boundary.
    append_n(&store, 105);
    let versions: Vec<u64> = store
        .read_range_iter(ReadQuery::stream("s"))
        .map(|r| r.unwrap().version)
        .collect();
    assert_eq!(versions.len(), 105);
    // Strict ascending order — no gaps, no duplicates across batch boundary.
    for (i, v) in versions.iter().enumerate() {
        assert_eq!(*v, i as u64, "version at index {i} should be {i}");
    }
}

// ── CorrelationIter ───────────────────────────────────────────────────────────

#[test]
fn correlation_iter_collects_all_correlated_events() {
    let store = temp_store();
    let root = append_correlated(&store, 6);
    let count = store.read_by_correlation_iter(root).count();
    assert_eq!(count, 6);
}

#[test]
fn correlation_iter_empty_returns_no_items() {
    let store = temp_store();
    store.declare_stream("c", "main", None).unwrap();
    let lone = store
        .append(Append {
            stream_id: "c".into(),
            branch: "main".into(),
            event_type: "Lone".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let items: Vec<_> = store.read_by_correlation_iter(lone).collect();
    assert!(items.is_empty(), "no events carry this correlation_id");
}

#[test]
fn correlation_iter_fused_after_exhaustion() {
    let store = temp_store();
    let root = append_correlated(&store, 1);
    let mut iter = store.read_by_correlation_iter(root);
    assert!(iter.next().is_some());
    assert!(iter.next().is_none());
    assert!(iter.next().is_none(), "fused");
}

#[test]
fn correlation_iter_across_batch_boundary() {
    let store = temp_store();
    let root = append_correlated(&store, 105);
    let items: Vec<_> = store.read_by_correlation_iter(root).collect();
    assert_eq!(items.len(), 105);
}

// ── CausationIter ─────────────────────────────────────────────────────────────

#[test]
fn causation_iter_forward_collects_descendants() {
    let store = temp_store();
    let ids = build_chain(&store);
    // root → d1 → d2 → d3 → d4 — iterator from root should yield 4 descendants
    let count = store
        .walk_causation_iter(ids[0], WalkDirection::Forward, 10, SamplingMode::Exhaustive)
        .count();
    assert_eq!(count, 4);
}

#[test]
fn causation_iter_empty_returns_no_items() {
    let store = temp_store();
    store.declare_stream("chain", "main", None).unwrap();
    let lone = store
        .append(Append {
            stream_id: "chain".into(),
            branch: "main".into(),
            event_type: "Lone".into(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();
    let items: Vec<_> = store
        .walk_causation_iter(lone, WalkDirection::Forward, 10, SamplingMode::Exhaustive)
        .collect();
    assert!(items.is_empty());
}

#[test]
fn causation_iter_fused_after_exhaustion() {
    let store = temp_store();
    let ids = build_chain(&store);
    let mut iter =
        store.walk_causation_iter(ids[0], WalkDirection::Forward, 1, SamplingMode::Exhaustive);
    // max_depth=1 yields only d1, then exhausts
    assert!(iter.next().is_some());
    assert!(iter.next().is_none());
    assert!(iter.next().is_none(), "fused");
}

#[test]
fn causation_iter_respects_max_depth() {
    let store = temp_store();
    let ids = build_chain(&store);
    let items: Vec<_> = store
        .walk_causation_iter(ids[0], WalkDirection::Forward, 2, SamplingMode::Exhaustive)
        .collect();
    assert_eq!(items.len(), 2, "max_depth=2 yields d1 and d2 only");
}

// ── Pool-release invariant ────────────────────────────────────────────────────

/// Verifies that iterators release the pool connection before yielding.
/// A store with pool_size=1 is used. A second thread reads concurrently while
/// the iterator is live. If the iterator held the connection across yields,
/// the second thread would time out (500 ms limit).
#[test]
fn iterator_releases_pool_connection_between_yields() {
    use std::sync::{Arc, Barrier};

    // pool_size=1 makes starvation immediate if the iterator holds the connection.
    let store = Arc::new(temp_store_pool(1));
    append_n(&store, 10);

    let barrier = Arc::new(Barrier::new(2));

    let store_ref = Arc::clone(&store);
    let barrier_ref = Arc::clone(&barrier);

    let handle = std::thread::spawn(move || {
        // Signal main thread that we're ready to read, then immediately read.
        barrier_ref.wait();
        let result = store_ref.read_range_bounded(ReadQuery::stream("s"), Some(1), None, None);
        result.is_ok()
    });

    let mut iter = store.read_range_iter(ReadQuery::stream("s"));

    // Advance the iterator one step (acquires + releases a batch internally).
    let first = iter.next();
    assert!(first.unwrap().is_ok());

    // Now release sync barrier — second thread will attempt a concurrent read.
    // The pool connection must already be released at this point.
    barrier.wait();

    // Consume the rest of the iterator to ensure it also releases correctly.
    for item in iter {
        item.unwrap();
    }

    let second_thread_ok = handle.join().expect("thread panicked");
    assert!(
        second_thread_ok,
        "concurrent read must succeed — iterator must not hold the pool connection across yields"
    );
}
