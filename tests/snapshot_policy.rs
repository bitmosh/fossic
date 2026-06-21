use fossic::{Append, Error, OpenOptions, Reducer, SnapshotPolicy, Store};
use serde::{Deserialize, Serialize};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

// ── Test reducer ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
struct SumState {
    count: u64,
    total: i64,
}

#[derive(Deserialize)]
struct AddEvent {
    value: i64,
}

struct SumReducer;

impl Reducer for SumReducer {
    type State = SumState;
    type Event = AddEvent;

    const NAME: &'static str = "sum_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> Self::State {
        SumState::default()
    }

    fn apply(&self, mut state: Self::State, event: &Self::Event) -> Self::State {
        state.count += 1;
        state.total += event.value;
        state
    }
}

fn append_add(store: &Store, stream_id: &str, value: i64) {
    store
        .append(Append {
            stream_id: stream_id.to_string(),
            event_type: "Add".to_string(),
            payload: serde_json::json!({"value": value}),
            ..Default::default()
        })
        .unwrap();
}

// ── Policy validation ─────────────────────────────────────────────────────────

#[test]
fn policy_invalid_zero() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    let result =
        store.register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(0));
    assert!(
        matches!(result, Err(Error::SnapshotPolicyInvalid(_))),
        "expected SnapshotPolicyInvalid, got {result:?}",
    );
}

#[test]
fn policy_not_implemented_seconds() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    let result =
        store.register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNSeconds(60));
    assert!(
        matches!(result, Err(Error::NotImplemented { .. })),
        "expected NotImplemented, got {result:?}",
    );
}

#[test]
fn policy_not_implemented_adaptive() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    let result = store.register_reducer_with_policy(
        "s1",
        SumReducer,
        SnapshotPolicy::StateAdaptive {
            target_replay_cost_us: 100_000,
            min_events_between: 10,
        },
    );
    assert!(
        matches!(result, Err(Error::NotImplemented { .. })),
        "expected NotImplemented, got {result:?}",
    );
}

// ── EveryNEvents behavior ─────────────────────────────────────────────────────

#[test]
fn every_n_events_no_snapshot_below_threshold() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store
        .register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(3))
        .unwrap();

    append_add(&store, "s1", 1);
    append_add(&store, "s1", 2);

    let _: SumState = store.read_state("s1", "main").unwrap();
    let snap = store.snapshot_info("s1", "main", "sum_reducer").unwrap();
    assert!(snap.is_none(), "no snapshot expected before threshold");
}

#[test]
fn every_n_events_snapshot_fires_at_threshold() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store
        .register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(3))
        .unwrap();

    append_add(&store, "s1", 1);
    append_add(&store, "s1", 2);
    append_add(&store, "s1", 3);

    let state: SumState = store.read_state("s1", "main").unwrap();
    assert_eq!(state.count, 3);
    assert_eq!(state.total, 6);

    let snap = store.snapshot_info("s1", "main", "sum_reducer").unwrap();
    assert!(snap.is_some(), "snapshot expected after threshold");
    assert_eq!(snap.unwrap().version, 2); // 3 events at versions 0, 1, 2
}

#[test]
fn every_n_events_counter_resets_after_snapshot() {
    // EveryNEvents(3): first read_state at 3 events fires a snapshot (v2);
    // second read_state at 3 new events fires another (v5).
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store
        .register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(3))
        .unwrap();

    for v in [1i64, 2, 3] {
        append_add(&store, "s1", v);
    }
    let _: SumState = store.read_state("s1", "main").unwrap();
    let snap1 = store
        .snapshot_info("s1", "main", "sum_reducer")
        .unwrap()
        .expect("first snapshot expected after 3 events");
    assert_eq!(snap1.version, 2);

    for v in [4i64, 5, 6] {
        append_add(&store, "s1", v);
    }
    let state: SumState = store.read_state("s1", "main").unwrap();
    assert_eq!(state.count, 6);
    assert_eq!(state.total, 21);

    let snap2 = store
        .snapshot_info("s1", "main", "sum_reducer")
        .unwrap()
        .expect("second snapshot expected after counter reset");
    assert_eq!(snap2.version, 5);
}

#[test]
fn manual_policy_never_auto_snapshots() {
    // Default register_reducer uses Manual — no snapshot should fire automatically.
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store.register_reducer("s1", SumReducer).unwrap();

    for v in [1i64, 2, 3, 4, 5] {
        append_add(&store, "s1", v);
    }

    let state: SumState = store.read_state("s1", "main").unwrap();
    assert_eq!(state.count, 5);

    let snap = store.snapshot_info("s1", "main", "sum_reducer").unwrap();
    assert!(snap.is_none(), "Manual policy must not auto-snapshot");
}
